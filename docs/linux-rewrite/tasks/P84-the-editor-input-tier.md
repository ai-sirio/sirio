# P84 — the editor input tier

**Owner: `codex11`.** Four rows, all in `tiller_ui/src/file_view.rs`, which is yours together with
`editor.rs`. **No seam in this pass** — you hold both halves of everything below.

Read `../ENVIRONMENT.md` and `../OWNERSHIP.md` first.

**The whole of this pass is mounting UI over a model that already works.** `editor.rs` is close to
complete and its API is listed below. If you find yourself writing buffer-mutation, conflict-detection
or preview-gating logic, stop — it exists, and rebuilding it is the specific mistake this brief is
written to prevent.

---

## What the critic drove, live, before writing this

On a disposable scratch Markdown file in the running app, opened via right-click → Open, in `Code`
mode:

| gesture | route | result |
|---|---|---|
| click a line | mouse | **works** — full-width current-line highlight |
| `Home`, `End` | key | **works** — caret moves |
| `shift+End` | key | **works** — selects the line |
| click `B` | mouse | **works** — wrapped exactly the selection, `X` → `**X**` |
| `ctrl-s` | key | **works** — the new bytes reached disk |
| type `TYPEDTEXT` | `xdotool type` | **nothing** |
| `key q`, `key w`, `key x` | `xdotool key` | **nothing** |

The editor navigates, selects, formats and saves. **It cannot accept a character.**

**It is not the harness.** Positive control in the same session: clicking the chat composer and
typing the same way put `HELLO FROM CRITIC` on screen and armed the send arrow
(`reference/linux-progress/orch13-composer-type.png`). Characters reach GPUI in this app today.

---

## Row 1 — character input (the defect; no inventory row owns it)

`grep` in `file_view.rs` for `on_key_down`, `KeyDownEvent`, `EntityInputHandler`, `key_down`,
`on_action` returns **zero hits**. The three `editor.insert(...)` callers in that file
(`:1049`, `:1092`, `:1137`) are all `#[cfg(test)]` fixtures. The model can insert; nothing in the
product ever asks it to.

**The precedent to copy is in this repo, not in a reference.** `chat.rs:1737` takes a
`&KeyDownEvent` and at `:1751` does:

```rust
} else if let Some(character) = event.keystroke.key_char.as_deref()
    && !event.keystroke.modifiers.platform
    && !event.keystroke.modifiers.control
```

That is the shape: read `key_char`, refuse it when a command modifier is held, insert otherwise.
Take the *approach*, write your own code — `chat.rs` is a composer with chips and a slash popup and
its handler is not transferable.

Must work, and each is a thing the critic will try:

- printable characters, including at start, middle and end of a line
- `Backspace` and `Delete`, including across a non-collapsed selection (`replace` already does this)
- `Enter` inserting a newline — **not** submitting anything; there is no submit here
- typing while text is selected replaces it
- the buffer going dirty so the existing `ctrl-s` path saves it — that path is proven, do not touch it
- the four gestures in the table above still working afterwards

`editor.rs:537 insert_indent` already exists, which suggests `Tab` was anticipated. Wire it if it
falls out naturally; it is not required and it is not a row.

**Do not add undo/redo.** `Editor` has no undo stack, it is not an inventory row, and inventing one
here turns a four-row pass into a rewrite.

---

## Row 2 — `F-EDIT-05`, the Reload/Keep banner

Ledger says `FAILED — absent`. **Only the banner is absent.** `editor.rs` has `check_external()`
(`:547`), `Conflict` (`:266`), `reload()` (`:595`) and `keep()` (`:620`), and
`file_view.rs:1075 conflict_detection_and_resolutions_work_through_the_view` already drives all of it
*through the view*, asserting "Reload adopted the on-disk content" and that Keep leaves the tab dirty
until saved.

So: draw the banner, give it **Reload** and **Keep**, call the two methods that already exist, and
make sure something calls `check_external` when the tab regains focus. The test that proves the
semantics is already green — extend it to the drawn control rather than writing a new model test.

## Row 3 — `F-EDIT-03`, manual preview on a large file

Ledger says "no manual-preview state; hardcoded 1 MiB notice instead". `editor.rs:692
request_preview()` and `:489 preview_locked()` both exist. The notice must become a real control:
when preview is locked, offer the user a way to render it anyway, call `request_preview()`, and show
the rendered document. `file_view.rs:172` is where the current request path lives.

## Row 4 — `F-EDIT-01`/`F-EDIT-02` must not regress

Both were moved to PASSED by live drive on 2026-08-14 and `F-EDIT-02` was upgraded from "renders" to
"exercised". The `Preview`/`Code` segmented control and the B/I/H/List/Link toolbar are the two
controls most likely to break when input handling lands, because they will now compete for focus and
for the selection. **Re-drive both before you report.**

---

## Not in this pass

`F-EDIT-12` (drag) is **contested and stays open**: `INTERACTION-TIER-AUDIT.md:101` says no file row
is a drag source and routes it to the shell, while your own P81 said the Changes list *is* a drag
source (`changes.rs:651`/`:992`) with the terminal as drop target
(`tiller_terminal/src/lib.rs:1187`) and a `(PathBuf, String)` payload. Two owners' documents
disagree about whether the feature exists. **Do not build it. If you can settle which is true from
the code in one look, write the answer into `ADJUDICATION-BACKLOG.md` and stop there.**

---

## Done means

1. A file you can open, **type into**, and save with `ctrl-s`, verified by reading the bytes back off
   disk — not by a screenshot, and not by a test alone.
2. `F-EDIT-05` and `F-EDIT-03` mounted over the existing model, with the existing tests extended
   rather than replaced.
3. `F-EDIT-01` and `F-EDIT-02` re-driven and still working.
4. `cargo test -p tiller_ui --lib`, clippy `-D warnings`, and `rustfmt` green on the files you touch.
5. `Scripts/transplant-check.py` run, with your files' status stated. 46 candidates are pre-existing.
6. `git status --short | grep '??'` before you call it done — **explicit-path commits never catch new
   files**, and that habit already left 31 MB of this project untracked once.

**The critic will open a file, type a sentence, save it, and `cat` it.** If the sentence is not in
the file, the pass failed regardless of what the tests say.
