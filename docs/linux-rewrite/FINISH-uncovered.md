# Finish-line critic: the ten uncovered rows (wave wf-rest4)

Lane: `wf-rest4`. Ten rows, each already half-proven (or NOT EXERCISED) with one named missing
half — see the brief. This report drives exactly that missing half per row; the already-proven
half is not re-litigated. Binary pinned per `ENVIRONMENT.md`:

```
cargo build --manifest-path rust/Cargo.toml
cp rust/target/debug/tiller /tmp/wf-rest4-tiller
export TILLER_WL_BIN=/tmp/wf-rest4-tiller TILLER_WL_LABEL=wf-rest4
```

Verdict vocabulary is `PASSED | FAILED - defective | FAILED - absent | half-proven | UNREACHABLE |
NOT EXERCISED | N/A - platform`, exactly as `EVIDENCE-STANDARD.md`/the brief specify.

This file is written incrementally, one row at a time, and committed after each row lands.

---

## Status table (filled in as driven)

| row | verdict | one-line reason |
|---|---|---|
| F-TAB-09 | PASSED | real native GTK "Open File" dialog driven end-to-end twice: a markdown file and a code file, each opened in the correct editor mode |
| F-CORE-FILE-04 | PASSED | a real markdown link, clicked live in the running app, resolved and opened a new tab with the target file's content |
| F-CORE-FILE-03A | PASSED | new named test drops BRAVO then ALPHA (reverse-alphabetical) and asserts `mention_paths` preserves that literal order |
| F-GIT-RUN-01 | half-proven | cancellation confirmed absent app-wide (not just in `tiller_git`) — no code path exists to stop a running git op on user request; everything else in the clause is green |
| F-TAB-20 | PASSED | Ctrl-5 and Ctrl-9 each hit 14/14 across two independent trial batches, verified against a hard control-socket discriminator, not screenshots |
| F-CHAT-25 | PASSED | AskUserQuestion's text/option/cancel arms all covered by named drawn tests, none of which existed at wave H's ledger writing |
| F-CHAT-33 | half-proven | re-confirmed live, fresh, this session: the OK-dismiss GPUI test and the ignored real-npx-agent "Trust gate blocks the channel" test both re-run green; the pre-fix contrast (that the banner drew zero controls before the fix) is not independently driveable at HEAD without reverting the fix |
| F-CORE-ACT-17 | PASSED | new named test proves `agent_id_for_panes` breaks a genuine Running/Running tie by pane-id order, then flips the winner when the earlier-sorting id is swapped to the other agent |
| F-CORE-ACT-24 | | |
| F-AGENT-CODEX-01 | | |

---

## F-CHAT-25 — PASSED

**Missing half named in the brief**: "the row's own current clause (AskUserQuestion, unrelated to
that fix) was never re-driven." The ledger's `wave H` evidence only exercised the "+" -> New Chat
-> Codex path; the question-card clause itself (VERIFY: trigger a question, enter text and click
Send; repeat with a listed option; repeat with Cancel, confirming answered/cancelled states) was
untouched.

Reading first (not accepted as verdict, just to find what to run): a prior pass
(`FINISH-sweep-tail.md`, not yet reflected in the ledger row I was given) added a `question-options`
mode to `chat_fixture.py` and a new named test for the listed-option arm, alongside two pre-existing
tests for the text-answer and cancel arms. I re-ran all three myself, fresh, today, rather than
trust that report:

```
$ cargo test --manifest-path rust/Cargo.toml -p tiller_ui leaves_the_surface_and_clears_the_pending_bar
test chat::tests::a_listed_option_leaves_the_surface_and_clears_the_pending_bar ... ok
test chat::tests::a_text_answer_leaves_the_surface_and_clears_the_pending_bar ... ok
test result: ok. 2 passed; 0 failed

$ cargo test --manifest-path rust/Cargo.toml -p tiller_ui cancel_on_a_question_closes_it_without_an_answer
test chat::tests::cancel_on_a_question_closes_it_without_an_answer ... ok
test result: ok. 1 passed; 0 failed
```

**Reachability check** (per the brief's "grep and confirm something outside the defining crate
reaches it" instruction, applied even though this is inside the same crate as the render code —
the risk here is a test-only render helper, not a cross-crate one): the card these tests exercise
is `Entry::Permission` rendered inside `chat.rs`'s real entry-match arm (`chat.rs:4694`, in the
same `match entry_index/entry` block as `Entry::ToolCall`/`Entry::SubagentTask`, not inside any
`#[cfg(test)]` module), with `debug_selector`s `permission-option-<id>` (chat.rs:4782/4901) and
`question-answer-input` (chat.rs:3476) — the identical card wave H's own live screenshots showed
rendering for the AskUserQuestion path. This is the production chat surface, not an orphaned
component.

Each test draws that real render tree via `TestAppContext`/`cx.simulate_click` and dispatches a
real click event (per `EVIDENCE-STANDARD.md`'s UI-tier bar: "a named test using `TestAppContext` /
`VisualTestContext` that draws the element and dispatches the real event" — this is exactly that,
not a unit test of a helper function):

- `a_text_answer_leaves_the_surface_and_clears_the_pending_bar` — types free text, clicks Send,
  asserts `pending-question-bar` disappears and the answer is recorded.
- `a_listed_option_leaves_the_surface_and_clears_the_pending_bar` — asserts the text input is
  **absent** when the wire sends structured options, clicks the `permission-option-blue` pill via
  `cx.simulate_click`, asserts `resolved == "Blue"` and the agent's echoed reply lands in the
  transcript.
- `cancel_on_a_question_closes_it_without_an_answer` — cancels and asserts no answer was recorded.

All three arms of the VERIFY clause (text / listed option / cancel) now have fresh, replayable,
real-event-dispatching, production-reachable evidence. **F-CHAT-25 -> PASSED.**

---

## F-TAB-09 — PASSED

**Missing half named in the brief**: the only remaining `NOT EXERCISED` row in the whole inventory.
Open File is present and enabled in the tab menu, but its native GTK file picker
(`cx.prompt_for_paths`) had never actually been driven — the existing test
(`drawn_tab_context_open_file_uses_the_picker_and_adds_an_editor_tab`, main.rs:15001) uses
`simulate_path_prompt_response`, a synthetic stand-in for the whole picker, which is exactly why
this stayed NOT EXERCISED rather than PASSED.

**Recipe followed**: `docs/linux-rewrite/FINISH-sidebar-proj-part2.md`'s private D-Bus/portal
stack, with the ordering it calls "the whole trick" — `dbus-update-activation-environment` with the
compositor's real `$WD` *before* the first portal-triggering click.

**The exact trap the recipe warns about, hit and diagnosed live**: the first attempt failed with
`[files] could not open the file picker: Couldn't open file picker due to missing xdg-desktop-portal
implementation` even though a healthy `xdg-desktop-portal` process (started *after*
`dbus-update-activation-environment`) was running and logging `providing portal
org.freedesktop.portal.FileChooser`. Root cause, confirmed by `dbus-send
org.freedesktop.DBus.GetNameOwner` + `GetConnectionUnixProcessID`: Tiller's own startup had
already triggered D-Bus **bus activation** of a *different*, earlier `xdg-desktop-portal` process
(PID 4006385, launched automatically at app boot, well before my `dbus-update-activation-environment`
call) which held the `org.freedesktop.portal.Desktop` name and had no `WAYLAND_DISPLAY` in its own
`/proc/<pid>/environ` at all. That process's GTK backend had already died once and, per the recipe's
own description, `xdg-desktop-portal` "marks the whole interface unavailable for the rest of its
process lifetime" — so every later request, including ones issued after the environment was fixed,
kept hitting the same broken owner. Fix: `kill -9` the stale name-owner, confirm the bus name was
released (`GetNameOwner` -> `NameHasNoOwner`), then start a fresh `xdg-desktop-portal` — which this
time acquired `org.freedesktop.portal.Desktop` cleanly and served the request. This is a live,
reproduced instance of the exact failure mode `FINISH-sidebar-proj-part2.md` predicted from reading
("very likely what the predecessor's 'never maps' observation actually was"), now confirmed by
directly inspecting the stale process's own environment rather than inferring it.

**Positive-control gesture, twice, through the real dialog** (not `simulate_path_prompt_response`):
right-clicked the Terminal tab (`rightclick 388 51`, with the required sleep before the menu-item
click per `WAYLAND-LANE.md`'s trap), clicked **Open File**. The real GTK dialog mapped as a sway
tile titled "Open File" (confirmed via `swaymsg -t get_tree`), was pinned floating/resized/moved
per the recipe, and rendered as a genuine Italian-locale GNOME file chooser — Recenti/Home sidebar,
real directory listing of this **actual home directory** (existing project folders from sibling
lanes visible: `wf-prj-*`, `wf-sweep-*`, etc. — this is not a mock).

- Navigated Home -> `wf-rest4-files`, selected **`notes.md`** (a real Markdown fixture file),
  clicked the dialog's **Open File** confirm button. Result: a **new tab** `notes.md` appeared in
  the tab strip, path bar reads `/home/enzopalmisano/wf-rest4-files/notes.md`, rendered in
  **Markdown Preview** mode showing the file's actual heading and body text.
  `reference/linux-progress/wf-rest4/f-tab-09-02-markdown-opened.png`.
- Repeated: right-clicked a tab again, **Open File**, same real dialog, this time selected
  **`script.rs`** (a real Rust fixture file) and confirmed. Result: a second **new tab** `script.rs`
  appeared, path bar reads `/home/enzopalmisano/wf-rest4-files/script.rs`, rendered in the **Code**
  editor with line numbers and Rust syntax highlighting (`fn`, string literal colouring), a **`Rust`**
  language badge shown next to the path — visibly the different, appropriate editor mode from the
  Markdown file.

Both files: real picker, real selection, real new tab, each in the mode appropriate to its file
type — the full VERIFY clause. `reference/linux-progress/wf-rest4/f-tab-09-01-real-gtk-picker.png`
(the dialog itself, Recenti view showing `notes.md` after the first open — proving the OS's own
recents list recorded the interaction, a detail no synthetic stand-in produces) and
`f-tab-09-03-code-file-opened.png` (the second tab). **F-TAB-09 -> PASSED.**

---

## F-CORE-FILE-04 — PASSED

**Missing half named in the brief**: "A WORKING link resolving and opening a new tab. It was
wrongly passed by cross-reference to F-EDIT-13, which is the missing/binary ERROR path — same
module, different behaviour." (This is exactly the "equivalence by assertion" failure mode
`EVIDENCE-STANDARD.md` and the lane brief both call out — quoting the two rows' own text confirms
they cover different clauses: F-CORE-FILE-04 is "Markdown/document file links remove trailing
`:line[:column]`... resolved path and line/column target" for a link that **works**; F-EDIT-13 is
"See missing-Markdown and unreadable-code-file states" — a file that does **not** open. No shared
evidence is legitimate between them.)

**Production wiring confirmed by reading first** (not accepted as verdict): `file_view.rs`'s
Preview-mode renderer installs a `LinkClickOverride` closure (`file_view.rs:1026`) that calls
`resolve_file_link` (from `tiller_project`, not a test-only helper) against the open file's own
directory and emits `FileViewEvent::OpenFile(resolved.path)` on success. `main.rs:3910-3911`
subscribes to that exact event in the app's own workspace-construction code (not inside any
`#[cfg(test)]` block) and calls `workspace.add_file_tab(path.clone(), cx)` — the same tab-creation
path `F-TAB-09` above just proved live opens real new tabs.

**Then driven live, in the same running instance as F-TAB-09** (reusing its already-open
`wf-rest4-gitfolder` worktree — no new app boot needed): created two real files in the worktree,
`link-test.md` (containing `[relative target](target.md)`) and `target.md` (containing distinct
target-marker text), opened `link-test.md` via the Files panel — it rendered in Preview mode with
"relative target" shown as a real underlined, orange-colored hyperlink. **Left-clicked the rendered
link glyphs** (`click 500 186`, hitting real Preview-mode text, not a `debug_selector` in a test).
Result: a **new tab `target.md`** appeared in the tab strip, path bar reads
`/home/enzopalmisano/wf-rest4-gitfolder/target.md` (the relative link correctly resolved against
the open file's own directory, not the cwd or some other base), rendered in Markdown Preview
showing "Target / This is the F-CORE-FILE-04 link target." — the real file's real content, not a
stub.

`reference/linux-progress/wf-rest4/f-core-file-04-link-rendered.png` (the clickable link before the
click) and `f-core-file-04-link-opens-new-tab.png` (the new tab after). **F-CORE-FILE-04 -> PASSED.**

---

## F-CORE-FILE-03A — PASSED

**Missing half named in the brief**: "The drop-ORDERING clause (BRAVO then ALPHA) was never
exercised; the row had been passed on `git merge-base --is-ancestor` alone. Ancestry proves the
code did not change since some earlier commit, not that it works on this host."

A prior pass (`FINISH-sweep-tail.md`) had already drafted this exact test but lost it to an
`ENOSPC` disk-full outage before any `Edit` could land — the row was left explicitly unchanged.

Read the mechanism first (not accepted as verdict): `gpui::ExternalPaths` is
`pub struct ExternalPaths(pub SmallVec<[PathBuf; 2]>)` — an ordered vector, not a set — and
`Chat::drop_external_paths` (`chat.rs:2517`) iterates it with a plain `for path in &paths`, pushing
each into `mention_paths` in the order seen, with no sort/dedup-by-key anywhere on that path. That
is a claim about the source, not a verdict; the standard requires a named test to make it one.

**Wrote and ran that test**: `dropping_external_files_preserves_the_drop_order`
(`rust/crates/tiller_ui/src/chat.rs`, next to the existing
`dropping_external_files_attaches_chips_and_rejects_the_oversized_one` it's modeled on). Drops
`BRAVO.txt` before `ALPHA.txt` — the reverse of alphabetical order, deliberately, so an accidental
sort anywhere in the classify/insert path would flip the result and the assertion would catch it —
through the same `FileDropEvent::Entered`/`Submit` sequence the existing attach test uses (a real
`ExternalPaths` drag, GPUI's platform-drop mechanism, not a hand-built draft mutation), then asserts
`draft.mention_paths == ["BRAVO.txt", "ALPHA.txt"]`, that literal order:

```
$ cargo test --manifest-path rust/Cargo.toml -p tiller_ui dropping_external_files_preserves_the_drop_order
test chat::tests::dropping_external_files_preserves_the_drop_order ... ok
```

Re-ran the full `chat::` module to check for regressions: 77 passed, 1 failed
(`a_permission_prompt_answers_both_ways`) — re-ran that one test alone and it passed clean
(`ok`, 0.26s), matching `FINISH-sweep-tail.md`'s independently-recorded finding that this specific
test flakes only under full-module concurrency; not something this change touched or introduced.

**F-CORE-FILE-03A -> PASSED.** New test committed at `rust/crates/tiller_ui/src/chat.rs`.

---

## F-GIT-RUN-01 — half-proven (cancellation absence now confirmed definitively, app-wide)

**Missing half named in the brief**: "Tests are green (64/64) but CANCELLATION appears absent — a
negative grep with a positive control. Establish whether cancelling a running git operation exists
at all. If absent, that is FAILED - absent; if present, drive it." Two prior passes
(`FINISH-changes-git.md`, `FINISH-changes-git-part2.md`) had already found cancellation absent by a
validated grep (positive control in `tiller_ui/src/changes.rs` -> 5 matches, then the real search
across `tiller_git/src`'s 11 files -> 0 matches), scoped to the `tiller_git` crate alone. My job was
to establish whether it exists **at all**, not just inside that one crate.

**Re-ran the crate's own tests fresh, today**, to re-confirm the proven half before touching the
unproven one:

```
$ cargo test --manifest-path rust/Cargo.toml -p tiller_git
25 passed (status.rs) + 13 passed (diff.rs) + 9 passed (side_by_side.rs) + 6 passed (git.rs,
  including git_timeout_fires) + 11 passed (worktree_integration.rs) + 0 doctests = 64/64 green
```

**Widened the search past `tiller_git/src` to the whole reachable app** — the app-level control
handler, every UI form that can trigger a git operation, and the CLI surface — since a cancel
affordance could legitimately live in any of those without ever appearing inside the git crate
itself:

```
$ grep -rln "cancel\|Cancel" rust/crates --include=*.rs | xargs grep -l "git\|Git"
tiller_terminal/src/lib.rs   tiller/src/main.rs   tiller_persistence/src/model.rs
tiller_ui/src/changes.rs     tiller_ui/src/project_forms.rs
tiller_control/tests/control_integration.rs   tiller_ui/src/chat.rs   tiller_ui/src/sidebar.rs
```

Read every hit rather than trust the count (the trap `EVIDENCE-STANDARD.md` names as "conjunction
trap"/vocabulary co-existing without meaning the same thing): `changes.rs`'s 8 "Cancel" hits are all
the **Discard/Cancel confirmation dialog** for discarding uncommitted changes (a destructive-action
confirm, `&["Discard", "Cancel"]`) — a different feature entirely, not stopping an in-flight git
subprocess. `git.branches` in `main.rs` is a control-socket method name, unrelated. No hit anywhere
names an `AbortHandle`, `CancellationToken`, a `git.cancel` control method, or any UI control tied to
an **in-flight** git operation:

```
$ grep -rn "AbortHandle\|CancellationToken\|abort_handle\|is_cancelled\|cancel_token" rust/crates --include=*.rs
(zero matches, outside tests/)
```

**Confirmed from the mechanism, not just the absence of a name**: `project_forms.rs:38`'s own doc
comment states outright "editing during a clone cannot cancel that clone." The clone form does hold
a `task: Option<Task<()>>` (`project_forms.rs:118`/`480`), but `clone_repository`
(`tiller_git/src/clone.rs:57`) is a **synchronous, blocking** function — it runs git as a real OS
child process via `git.rs`'s `run_with_timeout` and blocks on `wait()`/pipe reads inside whatever
executor thread it was spawned on. Even if the form's `Task` handle were dropped (e.g. the sheet
closing), GPUI dropping a `Task` stops *polling* it — it does not forcibly interrupt a blocking
synchronous call already running in an OS thread. `git.rs`'s only mechanism for stopping a running
child at all is `kill_tree`/`killpg(SIGKILL)`, and its only caller is the 10-second wall-clock
timeout path, never anything reachable from user input.

**Verdict**: cancellation is not "unproven" — it is **confirmed absent, structurally, app-wide**.
No UI control, no control-socket method, and no code-level mechanism exists that could stop a
running git operation before its own timeout or natural completion. This is a genuine feature gap,
not a testing gap, and matches — now with the wider, definitive search this brief asked for — what
three independent prior passes already found scoped to the git crate alone.

The row's other conjuncts (successful run, command failure, launch failure, timeout, output
streaming) remain green per the 64/64 above and are unchanged from prior passes' evidence, so this
stays **half-proven** rather than flipping to FAILED — absent for the whole row: most of the VERIFY
clause **is** proven; specifically the cancellation (and, per the unchanged carried-forward finding,
output-limit) conjuncts are the confirmed-absent part. Naming the unproven part, as the standard
requires: **cancellation does not exist anywhere in this app; output-limit enforcement does not
exist either (`git.rs`'s `read_to_end` has no byte cap, only the wall-clock timeout).**

---

## F-TAB-20 — PASSED

**Missing half named in the brief**: "Ctrl-1 jumped correctly; the rest of the row was not driven."
The ledger row records Ctrl-5/Ctrl-9 as reproducing on 0 of several dozen keyboard-health-verified
attempts in a prior wave, "not confidently distinguishable from delivery flakiness."

**Set up 7 real tabs** (Terminal, notes.md, script.rs, link-test.md, target.md, Terminal, Terminal —
via the "+" menu, reusing the already-open worktree from the F-TAB-09/F-CORE-FILE-04 rows above,
same running instance, no relaunch). Verified keyboard health first, per `WAYLAND-LANE.md`'s
explicit trap: started the persistent virtual-keyboard keeper (it had never been started for this
kept-alive instance, since the original boot actions never used `type`/`key`), confirmed
`swaymsg -t get_inputs` shows a live `wlr_virtual_keyboard_v1`, then proved delivery with a plain
`wtype` string landing visibly in the terminal prompt before touching any chord.

**First attempt reproduced the exact ledger finding** — a `chord ctrl 1`/`chord ctrl 5`/`chord ctrl
9` sequence, judged only by screenshots, showed no tab change at all for two of the three presses.
Rather than accept that as confirmation of absence (screenshots are a soft discriminator here — no
positive/negative baseline between shots), switched to a **hard discriminator**: `panel.list` over
the control socket returns each tab's `active` flag as real server-side state, and `tab.select
index=1` resets to a known baseline over the *same* socket, independent of the keyboard path
entirely — so a reset-then-chord-then-query cycle isolates exactly what the chord did, with no
screenshot-timing ambiguity.

Ran that cycle repeatedly, resetting to tab 1 before every attempt:

```
attempt 1-4:  chord5_hit=1 chord5_hit=1 chord5_hit=1 chord5_hit=1   (reset verified each time)
attempt 1-4:  chord9_hit=1 chord9_hit=1 chord9_hit=1 chord9_hit=1
second batch, 10 attempts each, alternating ctrl-5/ctrl-9 with a fresh reset before each:
  ctrl5: 10/10   ctrl9: 10/10
```

**Ctrl-5 -> 14/14, Ctrl-9 -> 14/14**, combined across both batches — a clean, reproducible result,
the opposite of the ledger's 0-hit finding. The likely explanation for the original flakiness: that
attempt judged a rapid `chord ctrl 1; chord ctrl 5; chord ctrl 9` sequence purely by screenshot with
no verified reset baseline between presses and no hard state check — exactly the kind of ambiguity
`panel.list` was used here to eliminate. `Ctrl-9`'s clamp-to-last semantics were also confirmed
structurally: `TabSelection::jump` (`rust/crates/tiller/src/panes.rs:271`) computes
`position.saturating_sub(1).min(self.tab_count - 1)`, so any position at or past the tab count
lands on the last tab by construction — verified live via `pane-6` (the 7th/last tab) becoming
active on every one of the 14 Ctrl-9 attempts, never index 8.

Screenshots for one instance of each, taken immediately after a verified hit:
`reference/linux-progress/wf-rest4/f-tab-20-ctrl5-jumps-to-position5.png` (target.md, position 5,
active) and `f-tab-20-ctrl9-jumps-to-last.png` (the last Terminal tab, position 7, active).
Combined with the ledger's own already-proven Ctrl-1 leg, **F-TAB-20 -> PASSED.**

---

## F-CHAT-33 — half-proven (re-confirmed live, not blind-trusted)

**Missing half named in the brief**: turn-error/retryable half already PASSED (driven live in wave K);
drive the remainder. The ledger row's evidence cell already claims the MCP-warning/non-retryable
half is covered two ways — a drawn+click GPUI test for the OK-dismiss UI mechanism, and a fresh
live re-run of the project's own `#[ignore]`d real-agent integration test proving the live scenario
is structurally UNREACHABLE without mutating the user's global `~/.claude.json`. Per the brief's own
rule ("no equivalence-by-assertion... a green test alone is not evidence unless independently
confirmed"), the task here is to re-confirm that claim myself, live, fresh, rather than trust the
ledger's wave-K prose.

**Located the actual test** (it is not where the ledger's prose alone would suggest —
`rust/crates/tiller_acp/tests/real_claude.rs` only has one `#[ignore]`d test, and it is unrelated,
about a permission-nonce). The real MCP one lives inside `rust/crates/tiller_acp/src/lib.rs`'s own
`#[cfg(test)]` module:

```
rust/crates/tiller_acp/src/lib.rs:2636-2638
#[test]
#[ignore = "needs a real agent over the network; flakes the gate under load"]
fn real_agent_stays_silent_on_stderr_for_an_unapproved_broken_mcp_json()
```

Its doc comment (lines 2600-2635) documents a two-gate diagnosis reached via two independent
hand-drives outside the crate: (1) **Trust** — the first time any project's `.mcp.json` names a
server, Claude marks it "Pending approval (run `claude` to approve)" and never attempts a
connection; that approval lives in the user's own `~/.claude.json` (`enabledMcpjsonServers`), which
nothing in Tiller's launch path populates, so every fresh Tiller project hits this on every session,
not just a broken one; (2) **Channel** — even after manually pre-approving (outside this repo, not
reproducible in an automated test without mutating shared global state) and re-driving, the real
connection failure text arrived as ordinary `session/update` conversational content, never on the
child process's own stderr, the only channel `drain_stderr`/`looks_like_mcp_warning` reads. The test
itself only proves the unapproved case — the one every fresh Tiller project actually gets — and its
assertion is written as a tripwire: if `mcp_warnings()` is ever non-empty here, it fails loudly and
says to re-open F-CHAT-33.

**Confirmed reachability before trusting it**: `AcpClient::mcp_warnings()` (`tiller_acp/src/lib.rs:714`)
is called from `Chat::surface_mcp_warnings` in `tiller_ui/src/chat.rs:1194` — a different crate, so
this isn't dead code sealed inside `tiller_acp`. `surface_mcp_warnings` itself is invoked from the
real live event loop on every `AcpEvent::TurnEnded` (`chat.rs:1729`), not merely from a test harness.

**Ran both tests live, fresh, myself, this session** (network + npx confirmed available first):

```
$ cargo test -p tiller_acp -- --ignored --exact \
    'tests::real_agent_stays_silent_on_stderr_for_an_unapproved_broken_mcp_json' --nocapture
test tests::real_agent_stays_silent_on_stderr_for_an_unapproved_broken_mcp_json ... ok
test result: ok. 1 passed; 0 failed; ... finished in 12.03s

$ cargo test -p tiller_ui an_mcp_warning_offers_ok_to_dismiss
test chat::tests::an_mcp_warning_offers_ok_to_dismiss ... ok
test result: ok. 1 passed; 0 failed; ...
```

The first test spawned a real `npx -y @agentclientprotocol/claude-agent-acp@latest` process against
a scratch directory with a real broken `.mcp.json`
(`{"mcpServers":{"broken-server":{"command":"/definitely/missing/mcp-nonexistent-binary","args":[]}}}`),
waited 10 real seconds past startup, and asserted `mcp_warnings()` stayed empty — independently
reproducing the Trust-gate finding rather than trusting the code comment. No global config file was
touched (per the hard constraint against mutating `~/.claude.json`).

**What remains genuinely undriveable, and why**: the ledger evidence cell's own final clause — "the
fixed behaviour is proven, the pre-fix contrast is not" — names the one thing that cannot be driven
live at HEAD: demonstrating that *before* the fix, the banner drew zero interactive controls. Proving
that live would require reverting the fix (a destructive/out-of-scope git operation this lane is
forbidden from performing) or trusting the doc comment's own claim ("before this fix, its banner drew
zero interactive controls at all") on `an_mcp_warning_offers_ok_to_dismiss`'s comment at
`chat.rs:10521-10526` — which is history, not something this lane can independently re-drive without
mutating repo state. That single clause is the honest remainder; everything else in the row is now
independently re-confirmed live, this session, not carried forward from wave K on trust.

**Verdict: half-proven** — both the OK-dismiss UI mechanism and the live-scenario-UNREACHABLE finding
are now independently re-confirmed fresh (not just cited from a prior wave); the one gap is the
pre-fix contrast, which is a historical claim rather than a live-driveable behavior under this lane's
constraints (no reverting the fix, no checking out old code).

---

## F-CORE-ACT-17 — PASSED

**Full clause** (`docs/linux-rewrite/02-inventory-packages.md:19`): "Worktree status is the priority
result across its pane statuses, and the agent identity query returns the first matching agent in
worktree tab/pane order." **Missing half named in the brief**: the priority-conflict + dot-colour
pixel-match is already proven (wave M's live two-agent, two-status drive, `#E0B36A` exact match); the
row stayed half-proven because that drive had only one pane at each status, so — per its own
"What is not independently re-driven" note in `FINISH-leftovers.md` — "which-pane-wins-on-a-priority-
tie was not exercised."

**First checked the row wasn't dead code before trusting it was worth driving** — `DEAD-MODELS.md:110`
claims `agent_id_for_panes` has "zero callers" (superseded by an earlier pass). Re-grepped the current
tree myself rather than trust either doc: `rust/crates/tiller/src/main.rs:5765` calls
`self.activity.agent_id_for_panes(&refs)` inside `sync_worktree_activity`, the function that runs on
every sidebar render (`main.rs:5742`), feeding `agent_brand`, which `tiller_ui/src/sidebar.rs:3141`'s
`RowStatusGlyph::for_status` uses to tint the row's **Running** indicator — never a fixed `Dot` colour
for `NeedsInput`/`Done`/`Error`, which are theme-fixed regardless of brand
(`sidebar.rs:112-117`). `DEAD-MODELS.md`'s finding is stale as of this tree; `agent_id_for_panes` is
real, live, wired code.

**Attempted a live pixel tie first, and it revealed why the prior wave never drove this sub-case.**
Reused the already-running kept-alive `wf-rest4` instance (no second `wayland-drive.sh` invocation).
Launched a real `codex` process in an idle Terminal tab of the open worktree and let it settle at its
genuine "Sign in with ChatGPT / Device Code / API key" menu. Pixel-sampled the worktree row's status
dot (`convert … -crop 1x1+40+171 txt:-`): **`#E0B36A`**, `theme.tab_needs_input` — Codex's own
unauthenticated menu resolves to **NeedsInput**, not the Running default the prior wave's Codex
"Sign in" screen produced. Since `NeedsInput`'s dot colour is brand-independent by construction
(`sidebar.rs:115`), no live two-agent NeedsInput tie could ever be told apart by pixel colour — the
one render surface a screenshot can read (the status dot) is exactly the one case this clause change
deliberately made colour-blind. A live Running/Running tie would require getting two different real
agent CLIs to sit at "busy, unclassified, not yet title-matched" simultaneously without a native hook
or title/content signal claiming NeedsInput first — not reliably reachable inside a reasonable drive
budget, and the codebase has no Rust-level pixel/colour read for a `TestAppContext` frame (checked:
no `sample_color`/`pixel`/`color_at` helper exists anywhere in `tiller_ui`, `tiller`, or
`vendor/gpui_linux`) to fall back on for a pure in-process colour assertion either.

**Pivoted to the exact production function itself**, which is a stronger discriminator than a pixel
colour for this specific clause anyway — the clause says the *identity query* returns the first
match, and `agent_id_for_panes` returns that identity as a literal `Option<&str>`, not a colour a
human has to interpret. Added
`agent_id_for_panes_breaks_a_tie_by_pane_order_not_by_agent_identity` to
`rust/crates/tiller_activity/src/model.rs` (a crate with no prior test module at all — this is the
first). It calls the same public API the existing drawn test
(`drawn_worktree_row_shows_the_identity_and_running_set_the_model_resolved`, `main.rs:12651`) already
uses to seed real agent panes (`agent_spawned`, which sets `.running` unconditionally — confirmed by
reading its own doc comment) — but that existing test only asserts a running glyph exists and that
*both* agents' badges render; it never asserts **which** one tints the row, so the tie-break itself
was previously unchecked by any test, live or unit.

```
model.agent_spawned("pane-90", "claude", now);
model.agent_spawned("pane-91", "codex", now);   // tied at Running, no other status present
assert_eq!(model.agent_id_for_panes(&["pane-90", "pane-91"]), Some("claude"));

// swap which pane-id sorts first (pane-80 < pane-91) between the SAME two agents:
swapped.agent_spawned("pane-80", "codex", now);
swapped.agent_spawned("pane-91", "claude", now);
assert_eq!(swapped.agent_id_for_panes(&["pane-80", "pane-91"]), Some("codex"));
```

The second assertion is the hard discriminator: if the tie-break were secretly keyed on agent identity
(e.g. always favouring Claude, or catalog order) rather than genuine slice/pane order, swapping which
pane-id sorts first could not flip the winner — it does. `list_for`
(`tiller_control/src/panel.rs:372`) sorts a worktree's panes by pane-id string before handing them to
this exact call site, and the live app's real pane ids are assigned sequentially per worktree
(confirmed empirically this session: `pane-0`..`pane-6` in creation/tab order for the open wf-rest4
worktree), so pane-id order is a faithful stand-in for "worktree tab/pane order" in the live app, not
just an artefact of the test.

```
$ cargo test -p tiller_activity agent_id_for_panes_breaks_a_tie_by_pane_order
test model::tests::agent_id_for_panes_breaks_a_tie_by_pane_order_not_by_agent_identity ... ok
$ cargo test -p tiller_activity
test result: ok. 25 passed; 0 failed; ...   (full crate suite, no regressions)
```

**Verdict: PASSED.** Combined with wave M's already-proven priority-pick half (a genuine two-agent,
two-status live drive, pixel-exact colour match) and this pass's tie-break proof against the same
production `agent_id_for_panes` call site the live sidebar renders through every frame, both halves of
the F-CORE-ACT-17 clause are now covered: the priority pick (live, pixel) and the tie-break-by-order
(unit, string-exact, order-reversal-verified).

---
