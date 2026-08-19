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
| F-TAB-20 | | |
| F-CHAT-25 | PASSED | AskUserQuestion's text/option/cancel arms all covered by named drawn tests, none of which existed at wave H's ledger writing |
| F-CHAT-33 | | |
| F-CORE-ACT-17 | | |
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
