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
| F-CORE-FILE-04 | | |
| F-CORE-FILE-03A | | |
| F-GIT-RUN-01 | | |
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
