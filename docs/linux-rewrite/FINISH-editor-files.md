# FINISH — editor-files shard (F-EDIT, F-CORE-FILE, F-CORE-DOM)

Lane: `wf-edit`. Machine: x86 desktop (`ENVIRONMENT.md` 2026-08-18 section), branch
`linux/gpui-waku`, HEAD at start of pass `99ac1f09`. Binary pinned: built
`cargo build --manifest-path rust/Cargo.toml` (warm, exit 0, 2 pre-existing dead-code warnings
only), copied to `/tmp/wfedit-tiller`, `TILLER_WL_BIN` pinned throughout. Lane driven via
`Scripts/wayland-drive.sh` against a nested sway on this box's own COSMIC/Wayland session
(`WAYLAND_DISPLAY=wayland-1`), never `DISPLAY=:1`.

Fixture: a throwaway git repo at `/tmp/wfedit-scratch` (own lane temp dir, not the real repo)
containing `scratch.md` (short markdown with bold/italic/list), `large.md` (400 019 bytes, over
the 256 KiB manual-preview threshold), `code.py`, `unknown.xyzext` (no known extension),
`binary.bin` (non-UTF8 bytes), plus `linktest.md`/`linktest2.md` created mid-pass to exercise a
markdown link to a missing vs. existing target. Every edit test restored the fixture with
`git checkout -- <file>` (or `git commit`/relied on a throwaway path) before the next test, so
disk state at the end of the pass matches the committed fixture content.

**Harness fact learned this pass, not previously documented as sharply**: each
`wayland-drive.sh` invocation restarts the app from scratch. The project/worktree list persists
(same SQLite DB, same `TILLER_WL_LABEL`), and — surprisingly — an open **Terminal** tab was
observed to persist/reopen across relaunches, but the app's own log is explicit that **editor
(file) tabs do not**: `[session] tab "code.py" (file) is not restorable in this build; skipped`.
Every multi-step editor test in this report was therefore driven as one continuous action block
inside a single `wayland-drive.sh` invocation (worktree-select → open → edit → save all in one
script), not chained across invocations.

**Double-click / right-click flakiness, reproduced and controlled for**: under this box's
ambient load (`uptime` ~9 on 12 cores throughout the pass, per `ENVIRONMENT.md`'s documented
concurrency ceiling), a `click A; click A` double-click or a `rightclick` occasionally produced
no effect at all — not a wrong effect, just a dropped synthetic input, consistent with
`WAYLAND-LANE.md`'s documented finding that a loaded box drops individual synthetic clicks. Every
such case was **re-run to an unambiguous positive** before being scored (never scored as FAILED
off a single dropped input, per project convention) — e.g. the F-EDIT-13 missing-file link click
returned nothing twice, then on identical coordinates opened the exact `does-not-exist.md` tab
with the specific message on the third attempt, and the control (a link to an *existing* file at
the same position) worked on first and second try, which rules out a real routing gap and pins
the two no-ops as dropped input.

## Rows selected (id prefix F-EDIT, F-CORE-FILE, F-CORE-DOM) — 30 total

F-EDIT-01..13 (13), F-CORE-DOM-01..08 (8), F-CORE-FILE-01..08 plus F-CORE-FILE-03A (9).

## F-EDIT — driven live, this pass

| id | verdict | evidence |
|---|---|---|
| F-EDIT-01 | PASSED | Double-click opened `scratch.md` in Preview (rendered H1, bullets, no line numbers); clicking `Code` switched to numbered raw source with the `**`/`*` markers visible and a `B I H List Link` toolbar. Screenshots `03-06-code-source.png`. |
| F-EDIT-02 | PASSED | Double-click-word then toolbar click, disk-verified: word `text` → `**text**` after Bold+ctrl-s (`cat scratch.md` showed `and *italic* **text** for editing.`, only that word wrapped); restored, then word `text` → `*text*` after Italic+ctrl-s (`and *italic* *text* for editing.`). Neither run touched the rest of the line. |
| F-EDIT-03 | PASSED | `large.md` (400 019 B, over the 256 KiB threshold) opened in Code with a `Large file — manual preview` bar and no auto-render (screenshot `02-60-large-loaded.png`, taken after a 3 s settle per the project's own "second-capture rule" for large renders); clicking `Preview` rendered it live (H1 `Large File Test`, `Preview` pill active, bar gone — `03-64-large-preview-click2.png`). |
| F-EDIT-04 | PASSED | Typed `WFEDITMARK07`, `ctrl-s`, then `cat`'d the file from the driving shell (outside the app): disk read back `WFEDITMARK07# Hello Scratch...` — the typed marker is on disk, hard discriminator independent of any screenshot. |
| F-EDIT-05 | PASSED | Both clause halves closed live with disk-external writes as the conflict source (`echo >> file` run directly from the drive script, not through the UI): clean tab auto-reloaded silently, no banner, new content visible in Code view. Dirty tab + external write produced the exact banner `This file changed on disk.` with `Reload`/`Keep`. **Keep**: buffer kept the local marker `ZZKEEPMARK`, did NOT show the external line `CLEAN-KEEP-EXTERNAL` (screenshot `05-44-after-keep-clean.png`). **Reload** (separate clean run): buffer replaced with disk content including the external line `CLEAN-RELOAD-EXTERNAL`, local marker `YYRELOADMARK` discarded (`05-52-after-reload.png`). |
| F-EDIT-06 | PASSED | `rm`'d the open file from the drive script (external delete while open): banner read exactly `This file was deleted. Saving will recreate it.`, no Reload/Keep. Typed `RECREATEDBYSAVE-F06`, `ctrl-s`; `ls`/`cat` from the driving shell confirmed the file existed again on disk (107 bytes) with the typed marker as its content. |
| F-EDIT-07 | PASSED | `code.py` opened with a `Python` pill and real syntax highlighting (keyword/string coloring); double-click-word `hello` → typed `renamed` → `ctrl-s` → disk read `def renamed():` with the 4-space-indented `print` line untouched (selection replaced exactly the word, indentation preserved). `unknown.xyzext` opened with a `plain text` pill (unknown-extension fallback). |
| F-EDIT-08 | PASSED | Double-clicked `scratch.md`'s Files-panel row twice in succession (two full double-click cycles); the tab strip showed exactly one `scratch.md` tab both times, no duplicate. |
| F-EDIT-09 | PASSED | Double-click on a Files-panel row (`scratch.md`) opened a new editor tab with path bar, mode pills, and rendered content — the same mechanism reused throughout this pass. |
| F-EDIT-10 | PASSED | Right-click on `code.py` drew a legible `Open / Reveal in File Manager / Copy Path` menu anchored at the pointer. `Open` opened the file in an editor tab. `Reveal in File Manager` launched a **real external process** — the COSMIC Files app opened a top-level window navigated to `/tmp/wfedit-scratch` with `code.py` pre-selected/highlighted (screenshot `03-93-after-reveal.png`) — a hard discriminator distinct from any in-app state. |
| F-EDIT-11 | PASSED | `Copy Path` on `code.py`, then pasted via the **terminal's own right-click → Paste** menu item (keyboard `ctrl+v`/`ctrl+shift+v` chords did not trigger a paste in this build's terminal or the sidebar filter field — see Gap below): the prompt read exactly `/tmp/wfedit-scratch/code.py` (screenshot crop `crop-prompt.png`), a real compositor clipboard round-trip (write via `cx.write_to_clipboard`, read back through the terminal's own paste path), not an in-process shortcut. |
| F-EDIT-12 | PASSED | `drag`ged `code.py`'s Files-panel row onto a live Terminal pane; the prompt received `'/tmp/wfedit-scratch/code.py'`, single-quoted — the production `on_drop` file-to-pane path, not a fixture. |
| F-EDIT-13 | PASSED | Both specific messages reproduced live. Missing: a markdown link `[missing file](./does-not-exist.md)` clicked in Preview opened a new tab whose body read exactly `This file does not exist: /tmp/wfedit-scratch/./does-not-exist.md` (screenshot `03-84-missing-multi-click.png`). Unreadable: opening `binary.bin` (non-UTF8 bytes) showed exactly `This file appears to be binary and cannot be shown as text.` (screenshot `02-70-binary-open.png`). |

### Gap noted, not a row regression
`ctrl+v` / `ctrl+shift+v` sent via `wtype` chords did not paste into either the terminal PTY or
the sidebar's GPUI `Filter` text field in this drive (tested with a real prior `ctrl+c`-copied
value and with a `Copy Path`-populated clipboard; both times the target field was unchanged
after the chord). The terminal's own right-click **Paste** menu item does work and was used to
close F-EDIT-11 live. This reads as a synthetic-input/keybinding-routing gap in this specific
harness path (or a real missing `ctrl+v` binding in one or both surfaces) rather than a clipboard
gap — the underlying clipboard write/read round-trip is proven to work by the menu-item path.
Not filed against any single row above since no VERIFY clause in this shard names the keyboard
chord specifically; flagged here for whoever next touches paste keybindings.

## F-CORE-DOM — status

(filled in next section of this pass)

## F-CORE-FILE — status

(filled in next section of this pass)
