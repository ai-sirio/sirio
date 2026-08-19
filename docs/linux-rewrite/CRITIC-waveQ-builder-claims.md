# CRITIC-waveQ-builder-claims — fresh critic over five builder-claimed fixes

Lane `wf-judge3`. Fresh critic, none of these five rows built by me. Per `EVIDENCE-STANDARD.md`,
only a critic may promote past `half-proven`, and only by exercising — so every verdict below is
backed by something I ran myself this pass, not a re-read of `FIX-waveO-failures.md` or the ledger.

Binary pinned from current HEAD (`da808f0a`): `cargo build --manifest-path rust/Cargo.toml -p
tiller`, copied to `/tmp/wf-judge3-tiller`, `TILLER_WL_BIN` set to it for every "on HEAD" drive
below.

**Pre-fix reproduction method.** `git archive <commit>^ | tar -x -C <tree>` for each fix commit's
parent, per the brief. To avoid three full from-scratch dependency builds (Cargo.lock is byte-
identical across all four fix commits and HEAD, confirmed with `git diff --stat <parent> HEAD --
rust/Cargo.lock` for each — empty diff every time), each pre-fix tree's `CARGO_TARGET_DIR` points
at a fresh `cp -al` hardlink copy of the already-built live `rust/target`. This reuses the compiled
dependency graph (registry crates are keyed by source, not by workspace path) and only recompiles
the ~13 local crates against the pre-fix source — verified this actually rebuilds the local crates
by grepping the resulting binaries for the fix's own added strings before trusting any of them as
"pre-fix". Three real, independently-built pre-fix `dev`-profile binaries resulted:

| pre-fix tree | commit^ | serves |
|---|---|---|
| `/tmp/wf-judge3-shared-target-c/debug/tiller` | `2c5c712c^` | F-SID-14 AND F-CORE-DOM-07 (confirmed the same commit carries both fixes — see F-SID-14 below) |
| `/tmp/wf-judge3-shared-target-a/debug/tiller` | `ea9026da^` | F-CORE-WSP-06 |
| `/tmp/wf-judge3-shared-target-b/debug/tiller` | `e846ca44^` | F-CORE-WSP-02 |

F-CORE-DOM-08 (`0b83f002`) has no live pre-fix *behavior* to reproduce — see its own section for
why, and what was reproduced structurally instead.

---

## F-CORE-WSP-02 — symlink resolution when deduping open document tabs

**Row** (`02-inventory-packages.md:38`, F-CORE-WSP-02, one clause of a larger row): "...document IDs
resolve symlinks and are worktree-scoped." VERIFY: "...reopen through a symlinked path, and confirm
the stable identity rules."

**Fixture**: `/home/enzopalmisano/wf-judge3-wsp02`, a real git repo with `note.md` (real file) and
`alias.md` (a real `ln -s note.md alias.md` symlink, confirmed with `ls -la` before driving).

### Side 1 — reproduced the original defect live, on the pre-fix binary

`TILLER_WL_BIN=/tmp/wf-judge3-shared-target-b/debug/tiller` (built from `e846ca44^`). Drove:
`project.add`, select worktree, double-click `alias.md` in the Files panel (two real `click`s at
the same coordinate — the row's own `event.click_count >= 2` gate, not a synthetic call), then
double-click `note.md`.

**Result: two separate tabs**, `alias.md` and `note.md`, both open simultaneously — screenshot
`/tmp/wj3wsp02pre-shots/02-02-after-opens.png` shows both in the tab strip, the active one's
breadcrumb reading `/home/enzopalmisano/wf-judge3-wsp02/note.md`. This is the defect the row
names, reproduced live on an independently-built pre-fix binary, not read from a commit message.

### Side 2 — confirmed the fix, live, on my own HEAD-pinned binary

Same fixture, same gesture sequence (double-click `alias.md`, then `note.md`), fresh label
(`wj3wsp02fix`), `TILLER_WL_BIN=/tmp/wf-judge3-tiller`.

**Result: one tab.** Screenshot `/tmp/wj3wsp02fix-shots/02-02-after-opens.png` shows a single
`alias.md` tab, breadcrumb `/home/enzopalmisano/wf-judge3-wsp02/alias.md` — the second double-click
(on `note.md`, the symlink's real target) reused the existing tab instead of opening a second one.
**Hard discriminator**, direct sqlite read of that drive's own database
(`/tmp/wj3wsp02fix.sqlite`):
```
tab: ('p-915dd749b8272428-wt-0-tab-18cd158d9d1783f7-0', 'p-915dd749b8272428-wt-0', 'alias.md', 'file')
```
Exactly one `kind='file'` row for the worktree, not two.

### Verdict: PASSED

Both sides independently driven this pass on binaries I built myself. The row's other clauses
(stable IDs for the other content kinds, worktree scoping) are not this row's live defect and are
covered by the "worktree scoping needs no new code" argument in the commit message, which I did not
re-derive — see gap note.

---

## F-CORE-WSP-06 — pane-split replay rejects a colliding `new_id`

**Row** (`02-inventory-packages.md:42`): "Layout validation rejects empty nonroot groups, orphan or
unresolved tabs/content, duplicate group/split/tab/content IDs, invalid active references, and
nonfinite or out-of-range fractions." VERIFY: "...construct snapshots containing each invalid
condition and confirm the app rejects or quarantines them rather than displaying corrupted layout."
This row's fix addresses exactly one of these conditions (duplicate pane/split IDs on replay); see
gap note for the others.

**Method**: real UI drive (`project.add`, worktree select, click **New Terminal**) to get a
persisted, real single-pane terminal tab, quit, then hand-edit that tab's `tab_state.state` JSON
directly in the sqlite file to append a second `Split` event reusing the same `new_id` as the
first — reproducing the exact "hand-corrupt the on-disk `tab_state.state` row" method the fix
commit itself used, done independently by me on my own fixture and my own pre-fix binary.

### Side 1 — reproduced the original defect live, on the pre-fix binary

`TILLER_WL_BIN=/tmp/wf-judge3-shared-target-a/debug/tiller` (built from `ea9026da^`). Wrote
`pane_events: [Split{focused:0,new_id:1,horizontal}, Split{focused:0,new_id:1,vertical}]` into the
real tab's persisted state, relaunched against the same database.

**Result: three visually rendered panes** from a tree that should only ever have two leaves —
screenshot `/tmp/wj3wsp06pre-shots/02-03-after-corrupt-relaunch.png` shows a 3-way split (two
stacked panes on the left, one full-height pane on the right). `panel.list` in the same drive
returned only **two** entries (`pane-0`, `pane-1`) — confirming the exact failure mode the commit
describes: a third live pane exists on screen with no reachable id in the id-keyed registry.

### Side 2 — confirmed the fix, live, on my own HEAD-pinned binary

Identical method: new fixture, new terminal tab, quit, same colliding-`new_id` corruption written
into its `tab_state.state`, relaunch on `/tmp/wf-judge3-tiller`.

**Result: exactly two panes**, cleanly split left/right — screenshot
`/tmp/wj3wsp06fix-shots/02-02-after-corrupt-relaunch.png`. `panel.list` returned exactly `pane-0`
and `pane-1`, matching the two visible panes one-to-one. The second, colliding `Split` event was
silently dropped rather than applied.

### Verdict: PASSED (for the clause this fix addresses — duplicate split/pane IDs on replay)

Both sides independently driven this pass, on binaries I built myself, using a corruption I wrote
myself rather than trusting the commit's own quoted transcript. Not re-verified by me: the row's
other quarantine conditions (empty nonroot groups, orphan tabs, invalid active references,
nonfinite fractions) — those are pre-existing behavior this fix did not touch, out of scope for a
critic pass whose brief is these five specific fixes. See gap note.

---

## F-SID-14 — agent-panel context-menu items target the wrong worktree

**Row** (`01-inventory-app.md:31`): "Open a terminal, agent panel, or chat for a worktree from its
context menu." VERIFY: "Right-click a worktree, choose New Terminal, a listed New agent Panel, and
New Chat in separate trials, and confirm the corresponding tab is created" [under the right-clicked
worktree — the specific defect this fix addresses].

### Ground truth on the shared commit, checked myself before trusting the builder's account

`FIX-waveO-failures.md` says the SID-14 production fix landed inside a commit titled only for
F-CORE-DOM-07. I verified this independently rather than taking the report's word for it:
```
$ git show 2c5c712c -- rust/crates/tiller/src/main.rs | grep -n NewTabForWorktree
38:+    NewTabForWorktree(PathBuf, NewTabAction),
46:+                                WorkspaceAction::NewTabForWorktree(path, action) => {
146:+                    actions.push(WorkspaceAction::NewTabForWorktree(path.clone(), action));
$ git log --oneline -S NewTabForWorktree -- rust/crates/tiller/src/main.rs
63f0f2ff test(F-SID-14): regression test for the agent-panel worktree race
2c5c712c fix(F-CORE-DOM-07): wire ChatEvent::TurnEnded into request_auto_rename
```
Confirmed: `2c5c712c` is genuinely the only commit that introduces `NewTabForWorktree`, so the same
pre-fix tree (`2c5c712c^`) serves both rows.

### Side 1 — structural reproduction; live race NOT independently reproduced, and I say so plainly

I ran `agent_panel_context_action_survives_a_worktree_switch_race` on HEAD (below) rather than
against the pre-fix tree, because the test itself was **added in `2c5c712c`/`63f0f2ff`** — it does
not exist in the pre-fix source at all, so "run the test against the pre-fix tree" is not available
as a probe (there is no test binary to build there for this function). The commit message quotes a
red/green pair obtained by reverting only the one queue-push line while keeping the new test; I did
not reconstruct that myself this pass — reproducing it would mean hand-patching the pre-fix archive
to graft in just the new test, and I judged that not worth the remaining time budget given the two
WSP rows and DOM-07 still needed live drives. **I did not attempt a live right-click race against
the pre-fix binary either** — the builder's own report states the original bug "was timing-dependent
and never reliably reproduced synthetically even when it was broken," so a live attempt with no
budget to retry many times would likely have produced a false negative (bug present, race not won)
that proves nothing. I am recording this gap rather than implying I reproduced the original defect.

What I *did* verify independently: `WorkspaceAction` in the `2c5c712c^` tree has no `NewTabForWorktree`
variant at all (only a bare `NewTab(NewTabAction)`), confirmed by grep on the archived source —
structural confirmation that the race the root-cause paragraph describes (queued action reading
ambient, mutable `working_directory` at drain time) was architecturally possible pre-fix, since
there was no path-carrying variant for the drain arm to use instead.

### Side 2 — confirmed the fix, on HEAD, by two independent means

1. **The regression test, run by me**, not just cited from the report:
   ```
   $ cargo test --manifest-path rust/Cargo.toml -p tiller --bin tiller \
       agent_panel_context_action_survives_a_worktree_switch_race
   test tests::agent_panel_context_action_survives_a_worktree_switch_race ... ok
   ```
2. **A live drive**, reusing the exact scenario `FIX-waveO-failures.md` already used (two
   worktrees, right-click the non-selected one, click Claude Code) would have been ideal to redo
   from scratch, but given the time budget I did not re-run this gesture myself this pass — the
   ledger's existing evidence for this exact scenario (`f-sid-14-correct-worktree-b.png` +
   sqlite `worktree_id` read) is wf-fix3's own live drive, already a real sqlite hard discriminator,
   not a screenshot-only claim, and I chose to spend the remaining live-drive budget on DOM-07's
   real-agent-turn drive instead, which had no cheap alternative.

### Verdict: half-proven

The fixed side has a test I ran myself (green) and — inherited, not independently re-driven by me
— a real sqlite-backed live drive from wf-fix3. What is missing for a clean `PASSED` from *this*
pass specifically is: (a) my own live re-drive of the worktree-targeting scenario, and (b) any
attempt, successful or not, at reproducing the original timing race live. Given the EVIDENCE-STANDARD
bar ("a verdict cannot outrun its transcript") I am not promoting to `PASSED` on inherited live
evidence alone, even though I have no reason to doubt it — the standard is explicit that only a
critic re-exercising promotes a row, and running someone else's screenshot back through my eyes is
not exercising it. A fresh critic with more budget should re-drive the worktree-targeting scenario
live and, if time allows, attempt the queue-race live several times as the report itself recommends.

---
