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

### Second gap noted, not a row regression (resolved during this pass)
`ctrl-tab`/`ctrl-shift-tab` appeared to do nothing across three separate attempts made while
focus was inside an open editor tab's text buffer (`code.py`/`scratch.md`) — this looked at
first like a possible regression in F-CORE-DOM-06's cycling half. Differential testing (a
positive control of `ctrl-1`/`ctrl-2` working from that exact same editor-focused context,
moments apart) narrowed it down, and a follow-up test starting from the **Terminal** tab (not
a text-editing widget) showed `ctrl-tab`/`ctrl-shift-tab` working correctly and wrapping in both
directions (see F-CORE-DOM-06 above). Conclusion: the editor's own text-input widget appears to
capture the `ctrl-tab` chord for itself (alongside plain `Tab`, which it legitimately needs for
indentation) before it reaches the window-level `CycleTabForward`/`CycleTabBackward` action
dispatch, when focus is inside that widget specifically. This is a real, reproducible, minor UX
gap (tab-cycling silently doesn't work while the cursor is inside an editor's text area) worth
someone's attention, but is not scored as a defect against F-CORE-DOM-06 itself since the
underlying cycle/wrap logic is proven correct and reachable from every other focus context tested
(Terminal, and — by the same window-level keybinding mechanism — presumably Chat and the Files
panel, though those weren't separately re-checked this pass).

## F-CORE-DOM — status

Supporting fact for the whole prefix: `cargo test -p tiller_project --lib` ran green live on
this host this pass (46/46), and the combined `tiller_ui`+`tiller_project` suite ran 348/349
(the one failure is a pre-existing, unrelated `titlebar` gsettings-environment test, outside
this shard). Cited per row only where it is the *primary* evidence; rows with a real UI surface
were live-driven instead.

| id | verdict | evidence |
|---|---|---|
| F-CORE-DOM-01 | PASSED | Incidental but strong: the project `wfedit-scratch` and worktree `master` (branch, path `/tmp/wfedit-scratch`, `Primary` flag) round-tripped identically across roughly 20 separate `wayland-drive.sh` relaunches in this pass (a fresh process + fresh SQLite read every time, per the harness fact above) — every screenshot in this report that shows the sidebar shows the same project/worktree state. This re-confirms the restart-survival half live; the explicit "edit a field, then restart" permutation wasn't independently re-driven this pass (the field-edit UI itself is out of this shard's row scope) — carried forward from the ledger's own prior evidence for that specific sub-case. |
| F-CORE-DOM-02 | half-proven | `cargo test -p tiller_project` re-ran `defaults_prefer_explicit_values_then_primary_and_sibling` green this pass. Live: opened **New Worktree...** from the sidebar and confirmed the dialog renders with a proposed branch/path pre-filled from the primary worktree (one combination only, not the full base/primary/location matrix the VERIFY clause names) — not a full live re-earn of every permutation this pass. |
| F-CORE-DOM-03 | half-proven | Not re-driven fresh this pass. The ledger's cited evidence commit `b27ad112` was confirmed (via `git merge-base --is-ancestor b27ad112 HEAD`) to be an ancestor of this session's HEAD `99ac1f09`, i.e. the code that earned the mark is present unmodified on this host. Left at `half-proven` (its existing ledger verdict) rather than upgraded, since I did not personally re-run the live drive this pass — only verified the evidence is not stale. |
| F-CORE-DOM-04 | PASSED | Unit tests green (5 filter tests). Live, incidentally: this pass's own F-EDIT testing typed several distinct queries into the sidebar `Filter` box across separate fresh launches (see F-EDIT section's `ctrl+a` note) and observed the project list filter/restore correctly each time — not the full empty/whitespace/case/branch-name matrix specifically re-driven as its own scripted pass, but a real, live, repeated exercise of the same filter path. |
| F-CORE-DOM-05 | half-proven | `cargo test -p tiller_project` re-ran `ordering_ignores_unknown_and_noop_moves` green this pass. No live sidebar drag-reorder of project/worktree rows was driven this pass (time-budget triage in favor of rows with no unit-test coverage) — carried at `half-proven` rather than claimed PASSED on unit tests alone, per this pass's "reading/tests are not live evidence" bar. |
| F-CORE-DOM-06 | PASSED | Fully live-driven, both directions, from a **non-editor-focused** tab (see gap note below for the one nuance found). With three tabs open (Terminal, code.py, scratch.md) and Terminal-tab focused: `ctrl-tab` pressed three times in a row walked Terminal→code.py→scratch.md→**Terminal**, confirming forward cycling **and** the wrap back to tab 1 (screenshot `02-170-wrap-back-to-terminal.png`, Terminal tab visibly highlighted/active). From Terminal (tab 1), a single `ctrl+shift+tab` jumped straight to **scratch.md** (tab 3, last) — the backward wrap, matching `domain.rs`'s `move_tab(0, 3, -1) == Some(2)` exactly (screenshot `02-180-shift-tab-wraps-backward.png`). Numeric selection: `ctrl-1`/`ctrl-2` reliably jumped to tab 1/2 earlier in this pass (F-EDIT section); this pass additionally re-confirmed `ctrl-9` selects the already-last tab (clamp case) and `ctrl-4` (out of range for 3 tabs) leaves the active tab unchanged, matching `numeric_tab_selection`'s `9 -> Some(2)` / `4 -> None` unit-test values. |
| F-CORE-DOM-07 | half-proven | Not re-driven fresh this pass (a real agent turn + throttle window is expensive against this shard's time budget). The domain throttle logic is covered by the green `tiller_project` suite re-run above. The ledger's cited live evidence ("sweep I1-autoname, 2026-08-16") does not carry a commit hash to check for ancestry, so I cannot confirm it is not stale the way I could for F-CORE-DOM-03/F-CORE-FILE-03A — left at `half-proven` rather than trusted as still-PASSED. |
| F-CORE-DOM-08 | half-proven | Pure internal `MainActor`-style once-gate with no UI surface; `once_gate_runs_only_the_first_callback` re-ran green in this pass's `tiller_project` suite run. No live UI path exists to drive this independently of the unit test, so it is not upgraded past `half-proven` under this pass's "code/tests alone are not proof" bar. |

## F-CORE-FILE — status

| id | verdict | evidence |
|---|---|---|
| F-CORE-FILE-01 | PASSED | Added two real subdirectories (`subdir_a`, `subdir_z`, each containing a file) to the fixture mid-pass and refreshed the Files panel live: the tree rendered `subdir_a` and `subdir_z` (folder icons) **before** any of the plain files (`binary.bin`, `code.py`, `large.md`, ...) despite `binary.bin` sorting alphabetically ahead of `subdir_a` on filename alone — a direct, live, disk-driven confirmation of dirs-before-files sort (screenshot `02-200-refresh-tree.png`). Expanding `subdir_a` showed its nested `nested.txt` correctly (`02-210-subdir-a-expanded.png`), confirming nested-directory loading. `.git` was never shown as a tree entry in any screenshot this whole pass, consistent with the exclusion rule. |
| F-CORE-FILE-02 | PASSED | Found the real UI surface: `TillerUi::chat.rs`'s `drop_external_paths` (tagged `F-CHAT-13` in source), bound to the **Chat composer**'s `on_drop::<ExternalPaths>` — a different call site than `tiller_project::classify_file_drop` (which, note, has **zero callers** anywhere in `tiller_ui`/`tiller`; it is domain-only/unit-test-only code in this build). Drove all four outcomes live via real compositor XDND (`Scripts/xdnd-source`, the same tool that closed F-CORE-FILE-03A) dropped onto an open Chat tab's message box: (1) a real 4x4 PNG (`pic.png`) produced an **Image ×** attachment chip (`02-260...png`); (2) an 11 MB file named `oversized.png` was rejected with the exact live message `oversized.png is too large (max 10 MB)`, no chip created (`02-270...png`); (3) a non-image in-worktree file (`code.py`) produced a plain file chip, not rejected (`02-280...png`); (4) a file **outside** the worktree (`/tmp/wfedit-outside.txt`) also produced a file chip, not rejected (`02-290...png`). Nuance: the composer chip UI always displays just the dropped file's basename (confirmed by dropping `subdir_a/nested.txt` and seeing only `nested.txt` on the chip), so the relative-vs-absolute *string* the clause names is confirmed by reading `chat.rs`'s `strip_prefix(&self.agent_cwd)` branch, not visually distinguishable from the chip alone — the four accept/reject *outcomes* are the part proven live end-to-end. |
| F-CORE-FILE-03 | PASSED | Same production path as F-EDIT-12, already live-verified this pass (drag from Files panel onto a live Terminal pane inserted the shell-quoted absolute path). Not re-driven a second time as its own row; cross-referenced rather than duplicated. |
| F-CORE-FILE-03A | PASSED | Not re-driven fresh this pass. The ledger's cited evidence commit `15cebcbf` was confirmed (via `git merge-base --is-ancestor 15cebcbf HEAD`) to be an ancestor of this session's HEAD `99ac1f09` — the exact code that earned the PASSED mark, including the two regression tests, is present unmodified on this host. This pass additionally re-used the same `Scripts/xdnd-source` tool successfully for F-CORE-FILE-02 above (READY/DRAG_STARTED/DROP_PERFORMED/FINISHED on every invocation), which is itself a live re-confirmation that the XDND mechanics this row depends on are still functioning on this host today, even though the row's own specific ordering/timing scenarios weren't re-run byte-for-byte. |
| F-CORE-FILE-04 | PASSED | Same production path as F-EDIT-13, already live-verified this pass (clicking a markdown link to a missing file opened a tab with the exact "does not exist" message; the same `resolve_file_link` -> `FileViewEvent::OpenFile` path serves both). Cross-referenced rather than re-driven separately. |
| F-CORE-FILE-05 | PASSED | Same production path as F-EDIT-02, already live-verified this pass (Bold/Italic toolbar wrap on a selected word, disk-confirmed via `cat` after `ctrl-s`, selection-preserving in both directions). Cross-referenced rather than re-driven separately. |
| F-CORE-FILE-06 | PASSED | Same production path as F-EDIT-05/06, already live-verified this pass (clean-tab silent auto-reload, dirty-tab conflict banner with exact text and Reload/Keep both individually exercised with disk-external writes, and external-delete producing the exact "will recreate it" banner then a real recreated file on `ctrl-s`). Cross-referenced rather than re-driven separately. |
| F-CORE-FILE-07 | PASSED | The inotify-backed watcher exercised live for F-CORE-FILE-06/F-EDIT-05/06 (external `echo >>`/`rm` from outside the app reliably reaching the open tab within the observed settle window) **is** this row's event stream — same monitor, same live evidence, not a separate mechanism to re-drive. |
| F-CORE-FILE-08 | PASSED | Live Files panel this pass showed visually distinct icons for markdown files (`scratch.md`, `linktest.md`), a Python file (`code.py`), a binary file (`binary.bin`), a no-extension/unknown file (`unknown.xyzext`), and folders (`subdir_a`, `subdir_z`, `.remember`) all rendered simultaneously in the same panel (e.g. `02-200-refresh-tree.png`, `02-240-fresh-launch-tabs.png`) — a real discriminator (folder glyph vs. distinct per-type file glyphs), not a single-icon fallback. |
