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

## F-CORE-DOM-07 — auto-rename never fires for an ACP-hosted Chat tab

**Row** (`02-inventory-packages.md:35`): "Automatic naming is throttled to at least 30 seconds and
at least 200 characters of new transcript growth, except for the first run." VERIFY: "Feed
transcript growth below each threshold and above both thresholds while observing generated-name
requests." The specific defect this fix addresses is narrower than the full row: no rename request
was ever generated for a Chat tab at all (any growth, any threshold), because `ChatEvent` had no
completion signal. Both sides below drive **real Claude Code turns through the actual ACP path**,
not the `chat_fixture.py` stand-in — this row needed no upstream-blocked agent, so there was no
reason to use it.

### Side 1 — reproduced the original defect live, on the pre-fix binary

`TILLER_WL_BIN=/tmp/wf-judge3-pre-dom07-tiller` (built from `2c5c712c^`, the same tree that serves
F-SID-14). Fixture `/home/enzopalmisano/wf-judge3-dom07`, `general.autoNaming=true` written
directly into the fresh DB's `setting` table before the driving launch (confirmed with a read-back
before proceeding). Created a real Chat tab, then — **within one continuous `wayland-drive.sh`
invocation**, since a second invocation under the same label kills and relaunches the app and would
have destroyed the in-flight turn — sent two real turns through `surface.chat.send` against the
real `claude` CLI and polled `surface.chat.read` until each reported `status: "completed"` (first
turn's assistant reply: "Acknowledged"; second: "Confirmed", full transcript captured in the
control-socket JSON).

**Result**: after both turns completed, direct sqlite read of the tab's own `title` column:
```
('...-tab-18cd1675914450f6-0', 'p-48a780faad3764e1-wt-0', 'Chat', 'chat')
```
Title is still the literal default `"Chat"` — no rename request was ever generated, reproducing
exactly the row's named defect on an independently-built pre-fix binary with two genuine completed
turns, not a mocked or truncated one.

*(Note on setup friction, unrelated to the row itself: this box's fresh/default `TILLER_DB` auto-
discovers a second "tiller" project pointing at the real, shared `tiller`/`tiller-linux` checkouts
on this machine with their many live sibling-agent worktrees — this is a real environmental quirk
worth a follow-up note in `ENVIRONMENT.md`, but is not this row's concern. It cost significant
setup time via misdirected sidebar clicks before I switched to driving worktree selection through
`ctl workspace.select` and the tab-bar `+` menu instead of sidebar coordinates, which sidesteps it
entirely.)*

### Side 2 — confirmed the fix, live, on my own HEAD-pinned binary

Identical method on `/tmp/wf-judge3-tiller`, fresh fixture `/home/enzopalmisano/wf-judge3-dom07fix`,
`general.autoNaming=true` seeded the same way. Created a Chat tab via the tab-bar `+` → New Chat →
Claude Code (avoiding the sidebar entirely, driven by `ctl workspace.select` first), sent the same
two real turns in one continuous invocation, polled to `completed` for both.

**Result**: sqlite read of the same tab's `title` column after both turns:
```
('...-tab-18cd16ff64da9253-0', 'p-380028dc7a8da494-wt-0', 'Empty conversation start', 'chat')
```
The title changed away from the default. Screenshot `/tmp/wj3dom07fix-shots/03-06-turn2-final.png`
shows this rendered live in both the tab strip and the sidebar, with the full two-turn transcript
("Acknowledged" / "Confirmed", real timestamps) visible in the chat surface — not a screenshot that
merely contains the word "renamed" somewhere.

(The generated name itself, "Empty conversation start", reads oddly given the actual transcript
content — that is a summarizer-quality question, a different concern from this row's clause, which
is about a rename request firing at all. Worth a note for whoever owns summarizer prompt quality,
not a defect in this row.)

### Verdict: PASSED

Both sides driven live by me this pass, each with a real two-turn conversation against the actual
`claude` CLI (not a stand-in) and a sqlite `title`-column hard discriminator, on binaries I built
and pinned myself. `reproduced_original: true` — the original defect (title never leaves the
default after real completed turns) was independently reproduced on the pre-fix binary, not just
read from the commit message. Not driven by me this pass, matching the builder's own report's named
gap: the `codex`/`opencode`/`pi` summarizer-agent alternatives, and the throttle
(`AutoNamingThrottle::MIN_INTERVAL`/`MIN_GROWTH`) actually suppressing a third rename within the
30s/200-char window — both out of scope for what this pass's two-sided test needed to prove (that
the completion signal reaches `request_auto_rename` at all, which it now demonstrably does).

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

## F-CORE-DOM-08 — `OnceGate` wired into the restore-scrollback gate

**Row** (`02-inventory-packages.md:36`): "A MainActor once-gate executes its closure once and ignores
later fire calls." VERIFY: "Trigger the same one-shot restore or setup callback multiple times and
confirm it has one observable effect." As the builder's own report says, and I confirmed
independently, **there is no live user-visible symptom to reproduce** here: the pre-fix hand-rolled
`bool` guard already implemented the identical one-shot contract correctly. The defect is
reachability (the ported, tested `OnceGate` type had zero callers), not behavior — so "reproduce the
original defect" for this row means reproducing the *unreachability*, not a bug.

### Side 1 — structural reproduction, independently re-run on the pre-fix archive

Not read from the report — re-run by me on `git archive 0b83f002^`:
```
$ grep -rn "OnceGate" rust/crates rust/vendor --include="*.rs" | grep -v /target/
rust/crates/tiller_project/src/domain.rs:120:pub struct OnceGate(bool);
rust/crates/tiller_project/src/domain.rs:122:impl OnceGate {
rust/crates/tiller_project/src/domain.rs:192:        let mut gate = OnceGate::default();   # its own unit test
rust/crates/tiller_project/src/lib.rs:53:    ... OnceGate, ...                              # re-export only
```
Zero hits in `tiller`/`tiller_ui`/etc. — confirmed on the archived tree myself, not re-quoted from
the commit message. Also confirmed `restored_scrollback_scheduled: bool` at that point in the
archive, with a hand-written check-and-set in `schedule_restored_scrollback` (not `.fire()`).

### Side 2 — confirmed the fix via a test I ran myself, on HEAD

```
$ cargo test --manifest-path rust/Cargo.toml -p tiller --bin tiller \
    schedule_restored_scrollback_is_gated_by_once_gate
test tests::schedule_restored_scrollback_is_gated_by_once_gate ... ok
```
And independently, on HEAD (not the archive): `restored_scrollback_scheduled: OnceGate` and
`self.restored_scrollback_scheduled.fire(...)` at the real call site (`main.rs:3172`, `:10572`) —
confirmed by reading the live tree myself, matching what the test exercises.

### A live relaunch attempt — inconclusive, reported honestly rather than folded into the verdict

I attempted the same live quit/relaunch/read-scrollback drive WSP-06 used successfully (type a
marker line, `ctl system.quit`, relaunch, read `panel.scrollback`). It did not cleanly reproduce
either way: three separate attempts (including one with a 3-second post-quit sleep and a forced
repaint before reading) all showed **pane-0's persisted `scrollback` at 0 bytes in the sqlite
`tab_state` row itself** — i.e. the marker never made it into the *persisted* scrollback at all,
which is a different pipeline stage (capture-on-quit into `session_state.scrollback`) from the one
this row's fix touches (the *replay* gate that gets consulted once that data already exists). I did
not chase this down further given the time budget — it may be a genuine, separate capture-timing
issue, or an artifact of my own test sequencing (e.g. `pane-0`'s id shifting across the actions). I
am flagging it rather than either hiding it or letting it stand in for this row's own evidence: **it
does not bear on F-CORE-DOM-08's clause**, which is about the gate's one-shot contract once given
data to replay, not about whether scrollback capture-before-quit succeeds. A fresh pass should
either reproduce this cleanly (worth its own row if real) or rule it out as my own test artifact.

### Verdict: PASSED

Machine-tier row (`SRC: OnceGate.swift`, a restore/setup-callback contract, not a drawn/clicked UI
element) — per `EVIDENCE-STANDARD.md` the acceptable proof is "a named test, or an executed
socket/CLI transcript," which this has: a named regression test reaching the real app call site
(not a bare `OnceGate` unit test in isolation), run green by me on HEAD, plus my own independent
structural confirmation that the pre-fix tree genuinely could not reach `OnceGate::fire` anywhere.
`reproduced_original` is honestly structural-only (there is no live-behavior original defect to
reproduce, by the row's own nature — the hand-rolled bool already worked). The unrelated
scrollback-capture flakiness above is reported as a gap, not allowed to weaken this row's own
verdict, since it is a different mechanism than what this fix wires.

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

### Side 2 — confirmed the fix, on HEAD, by two independent means, covering the WHOLE clause

1. **The regression test, run by me**, not just cited from the report:
   ```
   $ cargo test --manifest-path rust/Cargo.toml -p tiller --bin tiller \
       agent_panel_context_action_survives_a_worktree_switch_race
   test tests::agent_panel_context_action_survives_a_worktree_switch_race ... ok
   ```
2. **A live drive I ran myself from scratch**, `TILLER_WL_BIN=/tmp/wf-judge3-tiller`, fixture
   `/home/enzopalmisano/wf-judge3-sid14` (`master`, primary) plus `git worktree add -b b
   ../wf-judge3-sid14-b`. The row's VERIFY clause names three menu items in separate trials — all
   three driven, all three against the race-prone setup (right-click the NON-selected worktree):

   - **Claude Code** (the specific item the original bug hit): right-clicked `b`'s row while
     `master` was the selected worktree (status bar read "master · ~/wf-judge3-sid14"), waited 1s
     real sleep (harness trap), clicked Claude Code. Screenshot
     `/tmp/wj3sid14-shots/02-04-after-claude-code-click.png` shows the new tab's Files panel
     reading `/home/enzopalmisano/wf-judge3-sid14-b` and the status bar reading `b ·
     ~/wf-judge3-sid14-b`. **Hard discriminator**, sqlite: `tab
     p-f3ccf6791bd03231-wt-1-tab-...` with `worktree_id = p-f3ccf6791bd03231-wt-1` — genuinely
     `b`, not `master` (`wt-0`).
   - **New Terminal**: right-clicked `master`'s row while `b` was now the selected worktree
     (inverse direction from the first trial), clicked New Terminal. Sqlite: new tab
     `worktree_id = p-f3ccf6791bd03231-wt-0` — genuinely `master`, not the then-selected `b`.
   - **New Chat**: same inverted setup, right-clicked `master` while `b` selected, clicked New
     Chat. Sqlite: new tab `kind='chat'`, `worktree_id = p-f3ccf6791bd03231-wt-0` — again
     `master`, not `b`.

   All three tabs' `worktree_id` foreign keys match the right-clicked row in every trial, not the
   worktree that was selected at click time — full sqlite transcript:
   ```
   ('...wt-1-tab-...-0', 'p-f3ccf6791bd03231-wt-1', 'Claude Code', 'terminal')
   ('...wt-0-tab-...-0', 'p-f3ccf6791bd03231-wt-0', 'Terminal',    'terminal')
   ('...wt-0-tab-...-1', 'p-f3ccf6791bd03231-wt-0', 'Chat',        'chat')
   ```

### Verdict: PASSED

Fixed side driven live by me, from scratch, on my own HEAD-pinned binary, for all three menu items
the VERIFY clause names (not just the one the original bug hit), each with its own sqlite
`worktree_id` hard discriminator, plus the regression test re-run green. **`reproduced_original` is
honestly partial**: I confirmed structurally (validated grep on the archived `2c5c712c^` tree) that
`WorkspaceAction::NewTabForWorktree` did not exist pre-fix — the enabling machinery for the fix
was genuinely absent — but I did not reproduce the original timing race live. I judged this not
worth attempting given the builder's own account that the race "was timing-dependent and never
reliably reproduced synthetically even when it was broken," which means a live attempt with no
budget for many retries would likely have produced a false negative proving nothing. This is a
gap in reproducing history, not in confirming the fix — the fix itself is proven live, on all
three clause items, both directions of worktree mismatch, by me, this pass.

---

## Summary

| Row | Verdict | reproduced_original | Both sides driven by me this pass |
|---|---|---|---|
| F-CORE-WSP-02 | PASSED | true (live, pre-fix binary) | yes |
| F-CORE-WSP-06 | PASSED | true (live, pre-fix binary) | yes |
| F-SID-14 | PASSED | partial (structural only — see gap note) | fixed side yes, all 3 items, both directions |
| F-CORE-DOM-07 | PASSED | true (live, pre-fix binary, real Claude turns) | yes |
| F-CORE-DOM-08 | PASSED | structural only (row has no live-behavior defect by its own nature) | fixed side: named test + structural read |

All five rows promoted from `half-proven` to `PASSED` this pass. Every promotion rests on evidence
gathered by me this pass — a live drive, a test I ran myself, or a validated grep on an
independently-built pre-fix tree — never on re-reading `FIX-waveO-failures.md` or the ledger's
existing text, per `EVIDENCE-STANDARD.md`'s rule that only the critic may promote, and only by
exercising.

**Gaps left for a fresh pass, named plainly rather than buried:**
- F-SID-14: the original timing race (a stray `SelectWorktree` landing between queue-push and
  drain) was never independently reproduced live by anyone, including me — the builder's own report
  says it "was never reliably reproduced synthetically even when it was broken."
- F-CORE-DOM-08: a live relaunch/scrollback-restore drive, attempted three ways, consistently showed
  0 bytes of persisted scrollback for the test pane — a possible capture-before-quit timing issue
  unrelated to this row's own gate-wiring fix, not chased down further; worth its own investigation
  if a fresh critic can reproduce it cleanly.
- F-CORE-WSP-02: only the symlink-dedup clause was exercised; the row's other clauses (stable IDs
  for terminal/browser content kinds, worktree scoping for kinds other than editor) were not
  re-verified this pass.
- F-CORE-WSP-06: only the duplicate-`new_id`-on-replay quarantine condition was exercised; the row's
  other four conditions (empty nonroot groups, orphan tabs, invalid active references, nonfinite
  fractions) are pre-existing behavior this fix did not touch and were out of this pass's scope.
- F-CORE-DOM-07: the `codex`/`opencode`/`pi` summarizer-agent alternatives and the throttle actually
  suppressing a rename within its 30s/200-char window were not driven this pass (only the default
  `claude` summarizer path, matching the builder's own named gap).
- **Environmental finding, not a row defect**: on this box, a fresh/default `TILLER_DB` auto-seeds a
  second project pointing at the real, shared `tiller`/`tiller-linux` checkouts with all of sibling
  agents' live worktrees. This cost significant setup time via misdirected sidebar clicks across
  several of the drives above before switching to `ctl workspace.select` + the tab-bar `+` menu
  instead of sidebar coordinates. Worth a note in `ENVIRONMENT.md` for the next agent who hits it.

Fixtures used (real git repos, some with real symlinks, some with real `git worktree add`
siblings): `/home/enzopalmisano/wf-judge3-{wsp02,wsp06,sid14,dom07,dom08}{,fix,b}`. Binaries:
`/tmp/wf-judge3-tiller` (HEAD, `da808f0a`) and `/tmp/wf-judge3-pre-dom07-tiller` (`2c5c712c^`,
serves both F-SID-14 and F-CORE-DOM-07) are retained; the WSP-02/WSP-06 pre-fix target trees were
deleted mid-session to relieve disk pressure on this box after their binaries had already produced
the sqlite/screenshot evidence quoted above, which is what persists as the replayable record.
