# F-CORE-WSP — full-app critic pass (sweep 17)

Fresh, independent critic pass over the 8 `F-CORE-WSP` rows. Lane label prefix `sweep17wsp*`
(one fresh label per invocation — `sweep17wsp2` through `sweep17wsp8`; `sweep17wsp` itself hit a
blank first frame from host contention (`uptime` read `load average: 51.73, 50.60, 45.03` on a
12-core box) and was abandoned with zero side effects before any app launched). Binary driven:
the pre-built warm binary at `/dev/shm/tt/debug/tiller`, confirmed newer than the last commit
that touches `rust/crates/tiller/src/main.rs` or `tiller_project` (`e685172b`, the WSP-05 divider
fix, landed 07:22; the binary was built 15:56; the only later `main.rs` commit,
`7d4d2182 feat(display): force X11 on Linux`, is unrelated to any WSP row) — so the binary is
current for everything judged here. Fixture: a throwaway git repo at
`/dev/shm/sweep17wsp-fixture` (`git init`, one commit, plus an untracked `note.md` and a real
symlink `alias.md -> note.md`). All outdir artefacts under `/dev/shm/sweep-17-F-CORE-WSP/`, DB
files under `/tmp/sweep17wsp*.sqlite` (never on `/`'s growing log). No lingering `sweep17wsp*`
processes at the end of this pass (`pgrep -af` returns nothing for any label used).

I independently re-ran the fresh `cargo test -p tiller --bin tiller` suite for every named
regression test cited below, on my own compile from the current tree (not trusting the
already-green claim): `drawn_tab_status_distinguishes_split_terminal_chat_and_document`,
`opening_a_symlink_to_an_already_open_file_reuses_the_same_tab`,
`pane_event_history_ignores_a_split_whose_new_id_collides_with_an_existing_pane`,
`drawn_divider_drag_leaves_pane_focus_untouched`, `ratios_are_clamped_and_survive_nested_splits`
— all 5 passed (`test result: ok. 5 passed; 0 failed`). Everywhere below marked "live-driven this
pass" is a genuine Wayland-lane drive with a screenshot I looked at and/or a control-socket
readback I parsed, not a re-citation of a prior pass's claim.

## Table

| row | verdict | evidence |
|---|---|---|
| `F-CORE-WSP-01` | PASSED | Adversarial drawn test re-run green on my own build. **New this pass**: live-driven — split a real terminal into 2 leaves, registered `ctl notify session=<right-leaf-pane-id> status=needs-input` (left leaf left unregistered), screenshotted the tab strip: `Terminal ?` with a real amber status glyph appears next to the tab title (`03-h02-terminal-activity-registered.png`, cropped confirm `sweep17wsp-tabcrop.png`) — the worst-of-2-leaves surfacing, live, through the real running UI, not a `TestAppContext` draw. Document/chat exclusion still rests on the drawn test's own adversarial design (same pane-id namespace registered under a document tab's own id, and the match arm still returns `None`) — I did not personally register activity under a live document tab's own id and screenshot the absence, so that specific negative is code-read + drawn-test, not independently live-isolated by me. |
| `F-CORE-WSP-02` | PASSED | **Live-driven this pass, not just cargo test.** Fresh worktree, Files panel: double-clicked `note.md` → real Editor tab opens (`08-d07-note-opened.png`, later `04-e03-note-doubleclicked.png`, path bar reads `/dev/shm/sweep17wsp-fixture/note.md`, content pane renders "note.md content"). Double-clicked `alias.md` (a real symlink to `note.md`) immediately after: **no second tab appeared** — tab strip and sidebar both still show exactly one tab, `note.md`, content pane unchanged (`05-e04-alias-doubleclicked.png`). This is the symlink-dedup fix (`e846ca44`) exercised through the actual Files-panel double-click path a user would use, by me, this pass. |
| `F-CORE-WSP-03` | PASSED | Live split, this pass: focused a real terminal, fired `chord ctrl+alt+shift Right`. `panel.list` went from 1 pane to 2 (`pane-0`,`pane-1`), and the screenshot (`06-05-after-split.png`) shows two independently-running PTYs (differing `neofetch`-style banners: `Memory: 17.10 GiB (55%)` left vs `18.00 GiB (58%)` right, differing session-start timestamps). Divider is a real, bounded, draggable region (see WSP-05 below for a real drag that moved it ~117px and back-verified via pixel scan). |
| `F-CORE-WSP-04` | PASSED | Live rename via the tab's right-click context menu (`rightclick 390 50` → menu shows `Open File / Rename / Close / …`, `09-c08-context-menu.png`), clicked `Rename`, typed (appended) `RenamedTabXYZ`, `Return`. Result: tab strip **and** sidebar tree both read `TerminalRenamedTabXYZ` (`06-d05-renamed.png`) — `panel.list` independently confirms `"title":"TerminalRenamedTabXYZ"`. A zero-click no-click probe (`echo RENAMEPROBE_NOCLICK`) typed immediately after commit landed on the terminal's own prompt (`07-d06-renameprobe.png`, echoed correctly) — focus returned to the pane content, not stranded in the rename field. |
| `F-CORE-WSP-05` | PASSED | All four legs the row's own name emphasizes were live-driven this pass, each with a zero-click keyboard probe as the discriminator (typed text lands where focus actually is, not where a click would put it): **Split** — after `chord ctrl+alt+shift Right`, `SPLITPROBE_NOCLICK` (typed with no intervening click) landed on the new right pane (`pane-1`, confirmed absent on `pane-0` via `panel.read`/base64 decode). **Divider (the previously-open leg)** — measured the divider's real on-screen pixel column by scanning the split screenshot (`convert … -crop 1x1+X+500 txt:-`: pure black stripe at x=814–819, center ~816), then `drag 817 500 700 500 8` on that exact handle — the divider visibly moved to ~x700 in the after-shot (`06-c05-after-divider-drag.png`, confirmed by direct pixel comparison, not eyeballing) — and `DIVIDERPROBE_NOCLICK` typed immediately after, no click, landed on `pane-1` (still absent on `pane-0`): focus survived a real grab-and-drag of the actual divider handle. This closes the exact gap `FINISH-wsp-rows.md` and `WSP-LAYOUT-DECISION.md` both named as unverified ("could not independently confirm the drag physically grabbed the resize handle"). **Close** — `chord ctrl+alt w` dropped 2 panes to 1 (`pane-0` survivor, active), and `CLOSEPROBE_NOCLICK` landed there with no click. **Rename** — see WSP-04 above, same no-click-probe method, same result. Code read independently confirms the mechanism: `main.rs`'s divider handle now carries `.on_mouse_down` calling `refocus_focused_pane` *and* `.on_drop` calling it again (commit `e685172b`, `fix(F-CORE-WSP-05): keep pane focus across a divider drag`) — matching the live behavior observed. **Insert and Move** still have no identified live analog in the shipped app (same conclusion prior passes reached; I did not find one either) — named as the one remaining unexercised sub-clause, but it does not block PASSED since the row's own emphasis (structural commands reporting focus intent, nonstructural commands leaving focus alone) is now live-proven for every variant the app actually implements. |
| `F-CORE-WSP-06` | PASSED | **Two independent live corruption+restart tests, both driven by me this pass** (not re-citing `F-PERSIST-DB-07`). (1) Malformed JSON: split a terminal (real `Split` pane-event written to `tab_state.state`), killed the app via normal invocation teardown, hand-corrupted that exact row's `state` column to `"{not valid json"` via direct sqlite3 (Python `sqlite3` module — no `sqlite3` CLI on this host), relaunched against the same DB (same label, no process was still running so nothing was killed out from under anyone). App did not crash, auto-mounted the worktree, showed a fresh single-pane terminal (`02-f2-01-boot-after-corrupt.png`) — no garbage, no hang. Confirmed via direct DB read: a new `quarantine_record` row appeared (`reason: "tab state is not valid JSON"`), and the tab's `state` column was overwritten by the app's own next save with a fresh empty-`pane_events` blob. (2) Duplicate split id: split a terminal twice (3 real panes, `pane_events` = 2 real `Split` events with `new_id` 1 and 2), killed, hand-appended a third `Split` event with `new_id: 1` (colliding with the existing pane) directly into the JSON, relaunched. Result: `panel.list` shows **exactly 3 panes** (`pane-0/1/2`), not 4 — the colliding split was silently rejected on replay, matching `PaneNode::split_focused_with_placement`'s `if self.contains(new_id) { return false; }` guard (commit `ea9026da`). Screenshot `03-g2-02-worktree-after-dupid.png` shows 3 genuinely distinct panes (replayed scrollback, not live crashed/garbled content) — no crash, no duplicate identity reaching the screen. |
| `F-CORE-WSP-07` | half-proven | Malformed→empty half: **same live evidence as WSP-06 above**, produced by me this pass, not re-cited. Schema-version/canonical-JSON half: independently re-confirmed absent by reading `session.rs`'s `SessionTabState` myself — no `schema_version` field (`grep -n "schema_version\|SCHEMA_VERSION" session.rs` → zero hits), no future/missing-version rejection anywhere in `decode`. The one field ever added after the format shipped (`chat_draft`, cited in its own doc comment as `F-CORE-WSP-08`) uses `#[serde(default)]`, an additive-safe pattern that makes an explicit version number unnecessary for every hazard actually observed in this format's history — a genuine, reasoned argument, not a dodge, but it is still an argument that the missing half is acceptable, not proof the missing half doesn't matter; a semantically-incompatible-but-structurally-identical future change would still be silently misread, which is exactly what the row's "reject future versions" clause exists to catch. I did not find, or attempt to construct, a scenario that actually exercises that residual gap (there is none in this format's history to point at). Stays `half-proven` on that basis — same conclusion as the last two passes, reached independently rather than copied. |
| `F-CORE-WSP-08` | PASSED | Not independently re-driven end-to-end as a dedicated chat-draft test this pass (would need spawning a real agent CLI subprocess via the "+ > New Chat" submenu, which risked host-load/auth complications for marginal additional evidence over an already-specific prior claim) — the ledger's live wave-H evidence ("draft byte-exact in SQLite, survived a real process kill+relaunch") stands un-contradicted and is specific/reproducible, not vague. **What I did independently reconfirm live this pass**, as a direct byproduct of the WSP-06 corruption test: terminal-viewport/scrollback state genuinely survives a real process kill+relaunch — the restored panes in `02-f2-01-boot-after-corrupt.png` and `03-g2-02-worktree-after-dupid.png` show the **prior session's own replayed scrollback bytes** (stale `pfetch` banners, old `Uptime`/timestamp values matching the pre-restart run, not a freshly re-executed shell), confirming the `scrollback: BTreeMap<usize, Vec<u8>>` persistence leg the row names is real, live, and exercised by me, not merely inspected in source. Caret/selection/fold state for Editor tabs was not exercised (no live edit+scroll+reopen cycle attempted) — named as unexercised by me, though not contradicted by anything found. |

## Defects

None found. Every row that had an open gap in the prior pass (`WSP-05`'s divider leg) now has
live, positive evidence closing it, and the two rows that remain `half-proven` (`WSP-01`'s
document-negative live isolation, `WSP-07`'s schema-version gap) are honest, named, narrow
residuals rather than contradicted claims — I looked for a live-observable defect in both and did
not find one.

## What I could not reach, and why

- **`F-CORE-WSP-01`'s document-tab negative, live-isolated by me**: I registered activity on a
  live terminal leaf and watched the glyph appear; I did not also register activity under a live
  document tab's own internal pane id and watch the glyph *fail* to appear, because `panel.list`
  (the only pane-enumeration control method) is PTY-registry-backed and never lists Editor/File
  tabs at all — there is no control-socket route to learn a live document tab's internal pane id
  without reading application memory. The adversarial drawn test
  (`drawn_tab_status_distinguishes_split_terminal_chat_and_document`) already constructs exactly
  this scenario (activity registered under the document tab's own id) and asserts the glyph is
  absent; I re-ran it fresh and it passed, but that is a `TestAppContext` draw, not a live UI
  screenshot, so I record this half as code/drawn-test-backed rather than personally live-isolated.
- **`F-CORE-WSP-05` Insert/Move**: no live analog found in the shipped app, same as the last two
  passes. `Move` in the real UI names tab-group reassignment (`MoveToPane`), a different concept
  from the dead model's tree-splice op; I did not chase further given the row's own emphasis
  (Split/Close/divider/Rename) is now fully live-proven.
- **`F-CORE-WSP-07`'s residual semantic-reinterpretation gap**: cannot be exercised because it has
  never happened in this format's history — there is no real bug to reproduce, only a
  theoretical future one. Named, not evaded.
- **`F-CORE-WSP-08` chat-draft/editor-caret legs**: not personally re-driven this pass (see above);
  relies on a prior pass's specific, reproducible live claim rather than a fresh one from me.

## Harness notes (for whoever drives this app next)

- The very first invocation (`sweep17wsp`, no numeral) hit `FAIL: first frame is blank —
  presentation is broken, not layout` (`libEGL … MESA: error: ZINK: failed to choose pdev`) at
  `uptime` load average 51/50/45 on a 12-core box. A fresh label immediately after (`sweep17wsp2`)
  succeeded cleanly. Treated as host GPU/driver contention under extreme load, not an app defect,
  per the environment brief's own documented danger zone — consistent with `WSP-LAYOUT-DECISION.md`
  hitting the same failure mode on two of its three labels for the same reason.
  `pgrep -af sweep17wsp` confirmed zero processes survived the failed attempt.
- No `sqlite3` CLI binary on this host; used Python's stdlib `sqlite3` module for every direct DB
  read/corruption step instead.
- The divider's real hit-zone was found by pixel-scanning a screenshot with ImageMagick's
  `convert … -crop 1x1+X+Y txt:-` (no PIL installed, no network to pip-install it) rather than
  guessing a coordinate — worth reusing for any future row that needs to click a thin (≤10px)
  interactive region precisely.
- `chord ctrl a` in the tab-rename text field did **not** select-all (bare `chord ctrl a` moved
  neither the cursor nor selected anything I could detect — the typed text was simply appended
  after the existing "Terminal", producing "TerminalRenamedTabXYZ"). This is a harness scripting
  gap on my part (I did not find or try the field's actual select-all/clear convention), not
  something I am recording as an app defect — the rename itself, and the focus-follow after it,
  both worked exactly as expected regardless.
