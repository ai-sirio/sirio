# F-CORE-FILE — file-tree / file-content domain logic (9 rows)

Fresh, independent critic pass. Live-drove against the warm binary (`/dev/shm/tt/debug/tiller`) via
`Scripts/wayland-drive.sh`, label prefix `sweepFCF19*` (one label per invocation — the sequence
`sweepFCF19a` through `sweepFCF19z`, ~20 short invocations rather than one long one, because each
new UI surface needed a screenshot-and-recompute-coordinates step that can't happen mid-script).
Screenshots live under `/dev/shm/sweep-15-F-CORE-FILE/` (not committed — every frame cited below
was opened and read, not assumed).

Fixture: a throwaway git repo at `/dev/shm/sweepFCF19-fixture` seeded with directories
`.github`, `a-dir`, `docs`, `node_modules`, `src`, `tests`, `Z-dir` (mixed case, some named after
the original's special-cased folder names) and ~27 files covering `.md .rs .py .json .yaml .db
.pdf Dockerfile .html .png(real 1x1 PNG) LICENSE .log .zip .lock .css .sql .mp3 .env .sh
file2.txt/file10.txt(numeric-sort probe) é-note.md(accented-name probe)`, plus a
`link.md`→`target.md` pair for link resolution, `format-me.md` for the toolbar, and `watched.md`
for the external-change tests. Outside-worktree fixtures at `/dev/shm/sweepFCF19-outside/`
(an image, a file with an apostrophe+space in its name, a file with a non-ASCII name) and
`/dev/shm/sweepFCF19-order/` (`ZULU.txt`/`ALPHA.txt` for drop-order). I did **not** replay the
existing ledger's evidence text — every row below was re-driven live this pass with its own fresh
evidence, and where I disagree with the recorded verdict I say so with the reproduction attached.

I read every Rust module this section's SRC citations name (`tiller_project/src/file.rs`,
`file_link.rs`; `tiller_markdown/src/editing.rs`, `document.rs`, `file_events.rs`;
`tiller_ui/src/file_view.rs`, `right_panel.rs`, `chat.rs`; `tiller_terminal/src/lib.rs`) and, for
two rows where the ledger's own paraphrase looked suspicious, the **original Swift source**
still present in this checkout (`Packages/TillerCore/Sources/TillerCore/FileIconKey.swift`,
`FileTree.swift`, `FileDrop.swift`) to check the ledger's summary against the actual reference
behaviour rather than trust it.

## Verdict table

| row id | verdict | evidence |
|---|---|---|
| F-CORE-FILE-01 | FAILED — defective | Dirs-before-files and case-insensitive alphabetic sort both confirmed live: `.github, a-dir, docs, node_modules, src, tests, Z-dir` (dirs) then `.env, a-note.md, app.lock, ...` (files) in that order, `.git` never listed (`/dev/shm/sweep-15-F-CORE-FILE/02-02-tree-full.png`). But the row's own "sorts names localized" clause is live-disproved: with `file2.txt` and `file10.txt` both present, the tree renders `file10.txt` **before** `file2.txt` (`/dev/shm/sweep-15-F-CORE-FILE/02-02-tree-full.png`, rows at y=601/631) — plain lexicographic order, not the natural/numeric-aware order a real "localized" (Finder-style) sort produces. Root cause read in `Packages/TillerCore/Sources/TillerCore/FileTree.swift:82`: the original explicitly calls Swift's `localizedStandardCompare`; the Rust port's `read_directory` in `rust/crates/tiller_project/src/file.rs:216-223` sorts on a plain `.to_lowercase()` string comparison, which has no natural-number or diacritic-folding behaviour. (Same root cause also means the accented `é-note.md` sorts after every ASCII name by raw codepoint rather than folding near unaccented siblings — read from the same comparator, not independently re-driven this pass beyond the file2/file10 case.) |
| F-CORE-FILE-02 | PASSED | Real compositor XDND (`xdnd` action, not the in-process `drag`) onto the live Chat composer. In-tree PNG (`intree-photo.png`) → accepted, rendered as an `Image ×` chip (`03-03-intree-image-dropped.png`). Outside-worktree PNG (`/dev/shm/sweepFCF19-outside/outside photo.png`) → also accepted as an `Image ×` chip (`02-02-outside-image-dropped.png`; the underlying relative-vs-absolute classification is exhaustively unit-tested in `file.rs`'s own test suite, matching the contract exactly, and is exercised end-to-end by F-CORE-FILE-04's own link-resolution pass). An 11 MB `.webp` → rejected with the **exact** transient message `big.webp is too large (max 10 MB)`, no chip added (`03-03-oversized-dropped.png`). A plain `.txt` file → **accepted** as a generic mention chip `notes.txt ×` (`04-04-unsupported-dropped.png`) — see the note below; this is correct, not a defect. |
| F-CORE-FILE-03 | PASSED | Real 3-file compositor XDND drop (apostrophe+space name, non-ASCII name, and a plain file) in one drag directly onto a live Terminal pane; `panel.read` over the control socket returned the pane's actual PTY buffer, tail: `'/dev/shm/sweepFCF19-outside/it'\''s a test file.txt' '/dev/shm/sweepFCF19-outside/café report.txt' '/dev/shm/sweepFCF19-fixture/file2.txt'` — each path individually single-quoted, the apostrophe correctly POSIX-escaped as `'\''`, the embedded space preserved inside its quotes, the non-ASCII `café` preserved verbatim, all three joined by exactly one space, no trailing newline before the shell prompt resumes. |
| F-CORE-FILE-03A | PASSED | Two independent real-compositor-drag confirmations (not the GPUI-simulated unit test, which was already in the tree from a prior pass — see note). (a) Chat composer: `xdnd`'d `ZULU.txt` then `ALPHA.txt` in one drag, deliberately reverse-alphabetical — chips rendered in that literal order, `ZULU.txt ×` before `ALPHA.txt ×` (`02-02-drop-order.png`). (b) Same 3-file terminal drop used for F-CORE-FILE-03 above preserved the exact drag order (apostrophe-file, café-file, file2.txt) in the inserted command line, not alphabetised. |
| F-CORE-FILE-04 | PASSED | Fresh live click-through on my own fixture (independent of the pre-existing wave-P evidence, which I also opened and visually confirmed — see below): `link.md` in Preview shows `relative target` as a real underlined clickable link; clicking it opened a new `target.md` tab reading `Target / This is the F-CORE-FILE-04 link target (independent re-check).` — the real file's real content (`04-04-after-click2.png`). First attempt at the same click landed on the link (hand cursor visible) but produced no navigation — retried with more settle time and it worked; recorded as a harness/timing wrinkle, not an app defect (see Harness notes). I also opened and read the prior evidence `reference/linux-progress/wf-rest4/f-core-file-04-link-rendered.png` and `-opens-new-tab.png`: same mechanism, a different fixture, same correct result. |
| F-CORE-FILE-05 | PASSED | Live Code-mode toolbar test on `format-me.md`. Dragged a pixel-precise selection and clicked the `B` button: the wrap landed on **exactly** the selected byte range (`**make it bo**ld`, because my drag selection was 2 characters short of the full word — the app faithfully wrapped what was actually selected, proving byte-exact selection/operation binding rather than a fuzzy "whole word" heuristic) (`03-03-after-bold.png`). A second pass with a clean two-line drag selection and the `List` button correctly toggled `- ` onto **both** lines (`- # Format test` / `- select this phrase and make it bold`), with the selection visibly expanded to whole lines (`03-03-after-list.png`) — the "prefix selected lines... recalculating the selection range" clause, multi-line case. See Defects for a documentation-trail finding (not a functional defect) about which module actually implements this live. |
| F-CORE-FILE-06 | PASSED | Three real filesystem-driven states against one open `watched.md` tab, using literal `echo >>`/`rm` from the driving shell (not the UI) as the external actor. (a) Unmodified + external append → silent auto-reload: Preview shows both the original line and the appended line, no banner (`03-03-after-external-unmodified.png`). (b) Locally dirtied (typed text, `edited` badge visible) + external append → a real conflict banner reading exactly `This file changed on disk.` with `Reload`/`Keep` controls, local edit still intact underneath (`06-06-after-external-conflict.png`). (c) External `rm` while open → banner reads exactly `This file was deleted. Saving will recreate it.`, and the row disappears from the Files tree live in the same frame (`03-03-after-external-delete.png`). |
| F-CORE-FILE-07 | PASSED | Subsumed by the F-CORE-FILE-06 drive directly above: all three external-change scenarios (modify while clean, modify while dirty, delete) were detected and surfaced within the same ~3s settle with no manual refresh — the underlying watcher (inotify-based per the row's own PLATFORM note) is demonstrably live and working, not merely present in source. |
| F-CORE-FILE-08 | FAILED — defective | Live Files-panel screenshot of the fixture (25 files across 18 distinct extensions/exact-names, 7 named directories) shows only **three** distinct file glyphs total — one generic document icon shared by `a-note.md, app.lock, archive.zip, config.json, data.yaml, database.db, doc.pdf, Dockerfile, format-me.md, index.html, intree-photo.png (a real PNG!), LICENSE, link.md, notes.log, photo.png, run.sh's siblings, script.py, song.mp3, style.css` — plus a distinct gear glyph for `.env` and a distinct terminal glyph for `run.sh` (`/dev/shm/sweep-15-F-CORE-FILE/02-02-tree.png`, cropped in `/dev/shm/crop-tree-icons.png`) — and **one** generic folder glyph shared by every one of `.github, a-dir, docs, node_modules, src, tests, Z-dir` with zero name-based distinction. Confirmed at the code level, `rust/crates/tiller_ui/src/right_panel.rs`'s `file_glyph()`: it special-cases only shell scripts, `.git*`-family dotfiles, and `.env`/config dotfiles; every other extension — and every directory regardless of name — falls through to one shared `Icon::File` / `Icon::FolderFill`. The original `Packages/TillerCore/Sources/TillerCore/FileIconKey.swift` defines **~40** distinct per-extension icon keys (swift, python, rust, json, yaml, image, video, audio, pdf, archive, sql, database, docker, log, lock, makefile, …) and **15** distinct named-folder icon keys (`folderSrc, folderTests, folderDocs, folderNodeModules, folderDist, folderScripts, folderConfig, folderAssets, folderPublic, folderPackages, folderVscode, folderGit, folderLib, folderTools, …`); this port implements 3 of the ~40 file mappings and 0 of the 15 folder mappings. The row's own contract text — "selects exact filename or extension icons for files **and named icons for directories**" — is not met for directories at all, and is met for a small minority of files. |

## Note on F-CORE-FILE-02's "reject unsupported" clause — the ledger's paraphrase is the imprecise thing here, not the app

The row's summary text says the app should "reject unsupported/oversize items." Live, a plain
`.txt` file dropped on the Chat composer is **accepted** (as a text-mention chip, not an image),
and I initially read that as a live disproof of the row. It is not. I went to the original
`Packages/TillerCore/Sources/TillerCore/FileDrop.swift:40-54` to check, and the real reference
behaviour is: `classify` maps every input to exactly one of `.image`, `.file(path:)`, or
`.rejected(reason: .imageTooLarge)` — a non-image extension "falls through to `.file` with no
branch of its own" (the Swift source's own comment) and is **never** rejected; the *only*
rejection case that exists anywhere in the original is an oversized image. There is no
"unsupported extension" rejection in the reference app at all. The Rust port's live behaviour —
image accepted, outside-tree image accepted, oversized image rejected with an exact message,
plain file accepted as a mention — matches the true reference exactly. Per the project's own
"check scope claims against the reference" discipline, I'm flagging the ledger's paraphrase
("reject unsupported... items") as the misleading artifact, not the app.

## Harness notes (not app defects)

- **The Files-panel row click is a double-click, not a single click.** `rust/crates/tiller_ui/src/right_panel.rs:623-631` gates `open_file` on `event.click_count >= 2` — a single `click` action only focuses/highlights the row (confirmed live: `02-02-link-md-open.png` shows the highlighted row and an unchanged "No Terminals" center pane). Two `click` actions issued back-to-back register as a real double-click and open the tab (`02-02-link-md-dblclick.png`). Not a defect — this matches the row's own doc comment ("Files follow the inventory's double-click contract").
- **`xdnd`'s anchor point matters.** Anchoring the drag source's overlay at `(900,500)` reliably failed ("the button press over xdnd-source's surface never reached start_drag") across 4 retries on this heavily-loaded host (load average ~12); anchoring at `(200,200)` (a point clearly outside the app's own window chrome) worked every single time afterward, dozens of drags, zero failures. Recorded as a positioning/host-contention interaction with this specific harness script, not app behaviour — I never once saw the app itself reject or misroute a drag once `start_drag` fired.
- **Two separate single-file `xdnd` drops into the same terminal prompt do not get a space between them** — this looks like a concatenation bug at first glance but is not: each drop's insertion has "no trailing newline" by contract, so the cursor sits immediately after the first drop's closing quote, and the second drop's text lands right there. A single `xdnd` call carrying multiple files in one real drag (the actual multi-select-and-drag gesture a user performs) correctly space-joins them, as F-CORE-FILE-03's evidence above shows. Recorded as a methodology note for the next critic who reaches for two `xdnd` calls instead of one multi-file call.
- **F-CORE-FILE-04's link click needed a retry.** The first click landed precisely on the link (hand cursor visible in the frame) but produced no tab; the identical click repeated ~3s later worked cleanly. One data point, not reproduced as a pattern — recorded rather than asserted as a defect per the standard of proof.
- Getting to a live Chat composer at all required going through `+` → `New Chat ▸` → `Claude Code` specifically — clicking `Claude Code` directly in the top-level `+` menu instead opens a plain **terminal-hosted** agent pane (`NewTabAction::ClaudeCode` → `add_agent_tab`, a real PTY running the `claude` CLI's own TUI), a different code path from the declarative `Chat` entity (`NewChatAgent`/`add_chat_tab`, ACP-backed, the one with the mention-chip composer this section's rows actually need). Worth knowing for any other F-CORE-FILE/F-CHAT critic budgeting time for this.

## Defects

1. **F-CORE-FILE-01 — not a true "localized" sort.** See verdict table. Reproduction: create `file2.txt` and `file10.txt` in the same directory, open the Files panel — `file10.txt` renders above `file2.txt`. Root cause: `rust/crates/tiller_project/src/file.rs`'s `read_directory` sorts with `.to_lowercase()` string comparison; the original `FileTree.swift` uses `localizedStandardCompare`. Fix shape: replace the plain string comparator with a natural/numeric-aware comparison (and, ideally, real locale collation for the diacritic case too).

2. **F-CORE-FILE-08 — file-icon and folder-icon mapping tables are almost entirely unimplemented.** See verdict table. Reproduction: open any worktree with more than a couple of file types — everything that isn't a shell script, a `.env`/config dotfile, or a `.git*` dotfile renders the identical generic document icon, and every directory (however named) renders the identical generic folder icon. Root cause: `rust/crates/tiller_ui/src/right_panel.rs`'s `file_glyph()` implements roughly 3 of the original `FileIconKey.swift`'s ~40 extension mappings and none of its 15 named-folder mappings. This is a real, user-visible regression from the reference app's Files panel (compare any of the reference screenshots in `reference/shots/` showing a populated file list against a fixture tree in this pass), even though it is not a crash or data-loss bug.

## Unreachable / not exercised

Nothing in this section was left completely untested. The one clause I could only verify at the
unit-test level rather than pixel-for-pixel live (the exact *relative-vs-absolute path text*
inside an accepted image drop's chip — the chip UI shows a generic `Image ×` label, not the
resolved path string, for either an in-tree or an outside-tree drop) is covered by
`classify_file_drop`'s own exhaustive test suite in `file.rs` (which I read in full and confirms
byte-for-byte match to the row's contract text: relative when the canonicalized path is under the
canonicalized worktree root, absolute otherwise) plus indirect live confirmation via
F-CORE-FILE-04, which resolves and opens a relative link against the correct directory. I judged
this sufficient to call F-CORE-FILE-02 PASSED rather than half-proven, but flag the chip-label
limitation here for the record.
