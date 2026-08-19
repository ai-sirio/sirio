# F-EDIT — Documents and editors (13 rows)

Fresh, independent critic pass. Live-drove against the warm binary (`/dev/shm/tt/debug/tiller`)
via `Scripts/wayland-drive.sh`, label `sweep6edit`, across six sequential invocations (an app
restart does not keep editor tabs open — confirmed live, matching a prior pass's documented
finding — so the section was driven as boot → act → screenshot → repeat rather than one unbroken
script). Fixture: a throwaway git repo at `/dev/shm/sweep6edit-fixture` containing `scratch.md`,
`scratch2.md` (link targets), `toolbar.md`, `italic.md` (isolated Bold/Italic targets),
`save.md` (edit/save/close/reopen), `conflict.md`/`conflict2.md`/`conflict3.md` (clean-reload,
Keep, Reload), `deleteme.md` (external-delete recreate), `code.py` (language detection),
`unknown.xyzext` (plain-text fallback), `binary.bin` (unreadable), `large.md` (331 819 B, over
the 256 KiB manual-preview threshold), `longline.py` (a single 380-char line, for the "no
wrapping" sub-clause). Screenshots live under `/dev/shm/sweep-6-F-EDIT/` (not committed — the
deliverable is this report; every frame cited below was opened and read, not assumed).

I did **not** replay the existing ledger. Every row below was re-driven live this pass with its
own fresh evidence. Net result: I reproduce all 13 recorded PASSED verdicts — but I also found and
confirmed **one real, reproducible display defect that the ledger's evidence text never
mentions and would not have caught**: the Markdown Preview renderer does not apply inline
emphasis (bold/italic) at all. See "Defects" below — I am not flipping any row to FAILED over it,
but it is a genuine gap in "renders the correct Markdown" that a screenshot-literate pass should
have caught and didn't.

## Verdict table

| row id | verdict | evidence |
|---|---|---|
| F-EDIT-01 | PASSED | Live, both directions, twice: double-click `scratch.md` opened it in Preview by default (H1, bullets, clickable links rendered — `04-03-scratch-preview.png`); clicking `Code` (724,88) switched to numbered raw source showing the literal `**bold**`/`*italic*`/`[missing file](./does-not-exist.md)` markers (`05-04-scratch-code.png`); clicking `Preview` (674,88) switched back cleanly (`06-05-scratch-preview-again.png`). The mode-switch mechanic is real and correct in both directions. **But** see Defects: Preview's rendering of inline emphasis itself is broken. |
| F-EDIT-02 | PASSED | Two clean, isolated single-target tests, each disk-confirmed via external `cat` after `ctrl-s`. Bold: double-clicked the word `text` in `toolbar.md`, clicked `B` (346,131) → disk read `Editing the word **text** here for toolbar checks.` (`11-10-bold-applied.png`). Italic: double-clicked the word `caret` in a fresh file `italic.md`, clicked `I` (370,131) → disk read `Please make *caret* italic now.` (`07-06-italic-applied.png` in the second pass, after an earlier attempt on a shifted line gave an ambiguous result — see Harness notes). |
| F-EDIT-03 | PASSED | `large.md` (331 819 B, over the 256 KiB threshold) opened in Code with the exact bar `Large file — manual preview` and a `Render preview` control, no auto-render (`19-18-large-opened.png`). Clicking the `Preview` pill unlocked and rendered it live — `# Large File Test` as a real H1, bar gone, `Preview` pill active (`20-19-large-preview.png`). |
| F-EDIT-04 | PASSED | Opened `save.md`, typed a marker, `ctrl-s`; external `cat` (run from the driving shell, independent of any screenshot) read back `# Save Test EDITMARK04` — the edit landed on line 1 rather than line 3 due to a dropped/mistimed click, not a save defect (see Harness notes) — the save-and-retain mechanic is what's being verified and it worked. `chord ctrl w` closed the tab (confirmed gone from the tab strip in the very next frame, `14-13-after-close.png`); double-clicking `save.md` again in the Files panel reopened it showing the identical persisted content (`15-14-reopened-save.png`). |
| F-EDIT-05 | PASSED | All three sub-cases, each with a real external write from the driving shell (not through the UI) as the conflict source. Clean: appended a line to `conflict.md` externally while its tab was open and un-edited → content updated silently, **no banner** (`23-22-conflict-reloaded.png`). Dirty + Reload: dirtied `conflict3.md` with a local marker, appended externally → banner read exactly `This file changed on disk.` with `Reload`/`Keep` (`25-24-conflict3-banner.png`); clicking `Reload` (1197,134) discarded the local marker and adopted the disk content (`26-25-conflict3-after-reload.png`). Dirty + Keep: same setup on `conflict2.md`; clicking `Keep` (1260,134) discarded the *incoming* external line and preserved the local marker (`12-11-conflict2-after-keep.png`). |
| F-EDIT-06 | PASSED | Opened `deleteme.md`, `rm`'d it externally while open → banner read exactly `This file was deleted. Saving will recreate it.`, no Reload/Keep (`13-deleteme-deleted-banner.png` in the b1 pass; also `14-deleteme-deleted-banner.png` in the exact wording check). Typed a marker, `ctrl-s`; external `ls`+`cat` confirmed the file existed again on disk (67 bytes) with content `# Delete Test RECREATED_F06`. |
| F-EDIT-07 | PASSED | `code.py` opened with a `Python` pill and real syntax highlighting (keyword/string/number colouring, no mode pills since it's non-Markdown — `16-15-code-py-opened.png`). Word-select + type + `ctrl-s` replaced exactly one word and left the 4-space indentation of the edited line and its neighbours untouched, disk-confirmed (`    renamed 1` in place of `    return 1`). `unknown.xyzext` opened with a `plain text` pill and no highlighting (`18-17-unknown-ext-opened.png`). No-wrap: opened a fresh 380-character single-line file (`longline.py`) — it renders as one unbroken visual row that runs off the right edge of the viewport, not wrapped onto a second row (`03-20-longline-opened.png`). |
| F-EDIT-08 | PASSED | Double-clicked `scratch.md`'s Files-panel row a second time while its tab was already open: the tab strip still showed exactly one `scratch.md` tab, refocused, no duplicate (`08-07-refocus-scratch.png`). |
| F-EDIT-09 | PASSED | Double-click on a Files-panel row (`scratch.md`) opened a new editor tab with a path bar and `Markdown`/`Preview`/`Code` mode pills — the same mechanism reused for every other row in this pass (`04-03-scratch-preview.png`). |
| F-EDIT-10 | PASSED | Right-click on a Files row drew a legible `Open` / `Reveal in File Manager` / `Copy Path` menu anchored at the pointer (`04-03-menu-codepy.png`, `06-05-menu-unknown.png` — positions cross-checked twice, consistent). `Open` opened the file in an editor tab (`05-04-opened-via-menu.png`). `Reveal in File Manager` launched a **real external process** — `pgrep` from the driving shell caught `/usr/bin/cosmic-files /dev/shm/sweep6edit-fixture` running live, and the resulting window (captured in `07-06-after-reveal.png`) is a genuine COSMIC Files window navigated to the exact fixture directory with the right-clicked file (`italic.md`) highlighted — a hard discriminator, not an in-app state. |
| F-EDIT-11 | PASSED | `Copy Path` on `save.md` via the context menu, then pasted via the terminal's own right-click → `Paste` menu item (a real compositor clipboard round-trip, not an in-process shortcut): the prompt read exactly `/dev/shm/sweep6edit-fixture/save.md` (`08-07-after-paste.png`). |
| F-EDIT-12 | PASSED | `drag`ged `large.md`'s Files-panel row onto the live Terminal pane (real GPUI `on_drag`/`on_drop`, not `xdnd`): the prompt received `'/dev/shm/sweep6edit-fixture/large.md'`, single-quote shell-escaped — visibly distinct from F-EDIT-11's raw unquoted paste immediately above it on the same prompt line (`09-08-after-drag.png`). |
| F-EDIT-13 | PASSED | Both specific messages reproduced live, verbatim. Missing: clicking a Preview-mode link `[missing file](./does-not-exist.md)` opened a new tab whose body reads exactly `This file does not exist: /dev/shm/sweep6edit-fixture/./does-not-exist.md` (`06-05-missing-link-attempt2.png` — the first attempt was a dropped click, see Harness notes, retried to an unambiguous positive per project convention). Unreadable: opening `binary.bin` (non-UTF8 bytes) showed exactly `This file appears to be binary and cannot be shown as text.` (`21-20-binary-opened.png`). |

## Defects

**Markdown Preview does not render inline emphasis (bold/italic) — found this pass, not
previously logged.** Confirmed twice, independently, with zoomed crops of the actual pixels (not
inferred): `scratch.md`'s Preview shows `This is bold and italic text for editing.` with the
words "bold" and "italic" in the exact same upright, regular-weight font as the rest of the
sentence (`/dev/shm/sweep-6-F-EDIT/zoom-bold-italic.png`, a 4x crop). A second, cleaner isolated
case — `italic.md`, containing only `Please make *caret* italic now.` with no other markup —
shows the same thing: "caret" is not italicised at all (`/dev/shm/sweep-6-F-EDIT/zoom-italic-preview.png`,
a 5x crop). Headers, bullet lists, and links all render correctly and distinctly in the same
Preview surface (bold/larger heading text, real bullet glyphs, coloured/underlined clickable
links) — only the strong/em inline markers are silently dropped. This means F-EDIT-02's toolbar
correctly writes `**bold**`/`*italic*` into the Markdown source (disk-confirmed, see the row
above) but a user who clicks Bold or Italic and then switches to Preview to see the result never
sees any visual change at all. I am not overriding F-EDIT-01/F-EDIT-02 to FAILED for this — both
rows' literal VERIFY clauses (mode-switching works; the toolbar changes the Markdown source) are
demonstrably true and I proved them live — but this is a real, reproducible, previously-unreported
product gap that whoever owns the Preview renderer should see.

## Where I disagree with the recorded ledger

Nowhere on substance — I reproduce all 13 PASSED verdicts with independent live evidence. The one
place I go further than the existing ledger evidence text (`docs/linux-rewrite/INVENTORY-LEDGER.md`
wave H, 2026-08-14) is F-EDIT-01: its evidence string is "Preview/Code mode switch renders raw
markers correctly," which is about the *Code* side only and is true. It says nothing about
Preview's own rendering fidelity, so it isn't wrong — but nobody had looked closely enough at
Preview's own output to catch the bold/italic gap above, and I want that on record rather than
silently carried forward as an unqualified PASSED.

## Harness notes (not app defects)

- **Dropped synthetic clicks under host load**, consistent with this project's own documented
  finding (`docs/linux-rewrite/FINISH-editor-files.md`): the first attempt at clicking the
  "missing file" Preview link produced no effect at all (`05-04-missing-link-attempt1.png`); an
  identical second click on the same coordinates succeeded cleanly. A right-click on a Files row
  once produced no context menu at all, leaving a blind follow-up click to land on empty panel
  space — re-run cleanly on the next invocation. Per project convention, neither was scored as a
  defect off a single dropped input.
- **Code-view vertical layout differs between Markdown and non-Markdown files.** A Markdown file's
  Code mode draws an extra `B I H List Link` toolbar row that a non-Markdown file (e.g. `code.py`)
  does not, so the same y-coordinate that hits line 3 in a Markdown file's Code view hits line 1 in
  a code file's Code view. This caused one word-select/replace test on `code.py` to land on
  `return` instead of the intended `hello` — a harness targeting mistake once accounted for; the
  underlying mechanic (select word → type → save → indentation preserved) is unaffected and is
  the actual thing F-EDIT-07 asks to prove.
- **A stale compositor socket file** (from a prior invocation killed by an outer timeout mid-run)
  caused two invocations in this pass to fail at `exit 3` (compositor never came up) with
  `Unable to connect to /tmp/sweep6edit-sway.sock`. `rm -f` on the stale socket plus a short sleep
  before the next invocation resolved it every time; not an application issue.
- One full-section invocation was itself killed by an outer 2-minute wrapper timeout partway
  through (all 26 screenshots up to that point were still written to disk and are cited above
  without issue); later invocations used an explicit longer timeout and completed cleanly.

## Not reached / could not verify

Nothing in this section was left unreached. All 13 rows were driven live to a clean positive.
