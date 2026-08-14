# W10-chg evidence — Wayland lane (wavea-W10-chg)

Fixture: `/tmp/w10chg-fixture` — fresh git repo with `staged.txt` (staged M), `changed.txt`
(unstaged M), `untracked.txt` (untracked). Instance `TILLER_WL_LABEL=wavea-W10-chg`, socket
`/tmp/wavea-W10-chg.sock`. `workspace.select workspace=<id>` (not `id=`) is the param name that
actually switches the current workspace to the fixture.

## F-CHG-01 (ledger line 192, currently FAILED — absent)

Manifest says: reclassify — Changes is a first-class tab, not a toggle mode of the Files right
panel; verify the architecture claim rather than re-testing the retired toggle spec.

- `panel.list` after `surface.changes.open` returns panels `pane-0 tab=Chat`, `pane-1
  tab=Terminal`, `pane-2/3/4 tab=Changes` — i.e. **Changes shows up as a peer entry in the same
  `tab` field as Chat and Terminal**, not as a state of some other surface.
- Read `rust/crates/tiller_ui/src/right_panel.rs` (the Files sidebar): its header hardcodes the
  literal string `"Files"` (line ~641) with its own `close-right-panel` button, and the file
  contains exactly one substantive reference to the word "Changes" outside of comments — an
  unrelated `ActivitySurface` test fixture label at line 2041. There is no toggle
  state/enum/button anywhere in this file that switches its content between a Files mode and a
  Changes mode. The two comments that do mention both together (lines 126, ~) describe them as
  two separate surfaces that can each go stale, not as two faces of one panel.
- Captured `chg01-a-changes-tab.png` (Changes tab foregrounded via `surface.changes.open`) and
  `chg01-b-tab1.png`/`chg01-c-tab2.png`/`chg01-d-tab3.png` (`tab.select index=1..3`, walking
  Chat -> Terminal -> first Changes tab) at forced-repaint resolutions — three visibly distinct
  tab contents in the same tab strip, confirming Changes is reached by `tab.select` exactly like
  Chat/Terminal, the peer-tab model, not a right-panel mode switch.
- Captures: `reference/linux-progress/wavea-W10-chg/02-chg01-a-changes-tab.png`,
  `03-chg01-b-tab1.png`, `04-chg01-c-tab2.png`, `05-chg01-d-tab3.png`.

Claim: `exercised-working` for the reclassify claim — live `panel.list` plus the header/toggle
absence read in `right_panel.rs` both independently confirm the architecture pivot the manifest
describes: Changes is a first-class peer tab (alongside Chat/Terminal) and the Files sidebar is a
separate, non-toggling right panel. This is a triage/wording finding, not a rebuild of the
retired toggle — the row's literal "toggle within one right panel" spec does not correspond to
any code path that exists.

## F-CHG-05 (ledger line 196, currently FAILED — defective)

Manifest says: reclassify — prior evidence tried Space on a file (a documented no-op for files)
and never tried Enter, the actual open gesture (`on_file_key`, right_panel.rs:585-620: Enter
opens a file, toggles a directory; Space toggles a directory only).

- Selected the fixture workspace, brought the Changes tab forward, `click 1150 150` on the Files
  right panel to give it focus and select a row, then `key Down` x2 to move off any directory row
  onto a file row, then `key Return`.
- Established the pixel-diff instrument's zero-floor first: two consecutive same-resolution
  shots with **no action** between them (`chg05-ctrl-c`/`chg05-ctrl-e`, both 1715x972) diff to
  `compare -metric AE` = **0** — proves an unchanged frame reads as exactly 0, not noise.
- Same-session, same-resolution diff of the content area (crop `1715x932+0+40`, below the tab
  strip) between the post-click frame and the post-`Return` frame: **AE = 1,533,960** differing
  pixels out of 1,598,180 in the crop — essentially the entire content region changed, consistent
  with the Changes-list view being replaced by an opened file's content. The tab-strip crop
  (`1715x40+0+0`) also changed by a smaller but nonzero 1,148 px, consistent with a new tab label
  appearing.
- `panel.list` itself did not gain a new entry across click/Down/Return — but per the ledger's
  "Documents and editors" section this app has a separate document-tab concept from
  `panel.list`'s terminal-panel listing (`workspace.add_file_tab`, `main.rs:2738`, is what
  `RightPanelEvent::OpenFile` drives), so `panel.list`'s silence here is expected and not
  evidence against the open — consistent with the WAYLAND-LANE.md warning that not every state
  change surfaces through every socket read.
- Captures: `reference/linux-progress/wavea-W10-chg/02-chg05-b-clicked.png` (after click, before
  Enter), `02-chg05-c-after-enter.png` (after Down/Down/Return), `02-chg05-ctrl-c.png` /
  `04-chg05-ctrl-e.png` (zero-floor control pair).

Claim: `exercised-working` — the actual open gesture (Enter on a file row) was driven, and it
produced a large, discriminating, non-default visual change in the content area against a
proven-zero no-op baseline, consistent with `on_file_key`'s documented Enter-opens-file behavior.
I cannot read the pixels (text-only agent) so I cannot confirm the new content is specifically a
file viewer rather than some other large repaint; that residual sighted-confirmation gap is the
same one already on record for other `F-CHG` rows (e.g. F-CHG-06/22 in the E08 sweep).

## F-CHG-03 (ledger line 194, currently FAILED — defective)

Manifest says: reclassify — `right_panel.rs:698-736` already draws a real Loading/Retry state
(this is the **Files** right-panel error path, `files-loading`/`files-error`/`files-retry`, not
the Changes tab — confirmed against `01-inventory-app.md`'s own `F-CHG-03` VERIFY clause, which
names Files/Refresh/inaccessible-path, not Changes). Prior evidence never attempted an
inaccessible-path Retry trial.

- Read `refresh`/`ensure_tree_refresh` (`right_panel.rs:209-271`): the periodic 1s auto-refresh
  loop explicitly guards `if panel.refresh_error.is_none()` — once an error is set, the loop
  stops refreshing on its own, so recovery requires the Retry button, not the passage of time.
  This predicts a discriminating, code-grounded control: restoring permissions without clicking
  Retry must leave the error state stuck.
- With the fixture workspace open and its Files panel showing a healthy tree,
  `chmod 000 /tmp/w10chg-fixture` (an inaccessible path, exactly the manifest's recipe) was run
  **inline inside the action block** (the block reaches `eval`, so plain shell commands work
  alongside `ctl`/`click`/`shot`), then 1.5s settle, then captured.
- **Stuck-without-Retry control**: restored `chmod 755` but issued **no click**, then captured —
  full-frame `compare -metric AE` against the still-broken frame = **0**, byte-identical. This
  independently confirms the code read above: permissions alone do not recover the panel.
- **Retry click**: same run, `click 1512 500` (roughly the centre of the Files panel body, where
  the centred error card renders) after restoring permissions. Full-frame diff against the
  pre-break OK state (a separate run, same fixture) = 5,471 — small, consistent with two
  independent app instances of the *same* content differing only in incidental repaint noise.
  Full-frame diff of that same post-click frame against the broken-permission error frame =
  76,506 — large, of the same order as the direct OK-vs-error diff (78,401). Together these two
  numbers place the post-click frame next to "OK", not next to "error": the click recovered it.
  A follow-up scan of further clicks at the same point confirms it lands: colour-count jumped
  from 8,120 (error) to ~9,100-9,125 immediately at the first click and stayed there.
- **Self-caught measurement mistake, left in for the trail**: an earlier attempt measured only a
  small, hand-picked crop (`405x898+1310+74`, my guess at the panel's on-screen bounds) and got a
  misleadingly small error-vs-after-click diff (218) that looked like Retry had failed. The
  full-frame comparison above contradicts that — the crop coordinates were simply wrong, not the
  app. Lesson applied: verify a suspicious small-crop result against a full-frame diff before
  trusting it.
- **Loading flash**: not caught. `read_tree`'s failure on a permission-denied root is a single
  fast syscall failure, not a slow walk, so `refresh_started` is true for a sub-frame duration —
  consistent with the original P104 evidence ("Refresh produced neither loading state..."). This
  remains unobserved on this lane; the loading-state code path itself (`right_panel.rs:698-708`)
  is unconditionally present and code-verified, only its live visibility is unproven.
- Captures: `reference/linux-progress/wavea-W10-chg/02-chg03-h-ok-a.png` (OK),
  `02-chg03-scan-err.png` (error, post-chmod-000), `03-chg03-scan-1.png`/`04-chg03-scan-2.png`
  (post-Retry-click, recovered), `05-chg03-k-err-b.png` (error at 1400 res, same-run control).

Claim: `exercised-working` — the inaccessible-path recipe the manifest asked for was driven
end-to-end: broke access, captured a genuine error state (confirmed nonzero vs OK), showed it
does not self-heal (byte-identical control), then recovered it with a real click on Retry
(large diff back toward OK, both by full-frame comparison and colour count). The one owed piece
is the transient Loading flash, which is a code-verified but not live-captured detail, not a
defect in the Retry mechanic this row is actually about.

## F-CHG-18 (ledger line 209, currently NOT EXERCISED)

Manifest says: exercise — drag source (`changes.rs:993`, real `(PathBuf,String)` `on_drag`
payload) and drop target (`tiller_terminal/src/lib.rs:1448`, production `on_drop` wired to
`receive_diff_drop`) are both real, non-test code; prove via a real-mouse-event recipe or route
to `DISPLAY=:1`, which this slice is barred from.

- Re-read `changes.rs:993` directly: `on_drag(payload, move |_, _, _, cx| cx.new(|_| ...))` sits
  in the production row-render closure (not behind `#[cfg(test)]`), confirming the manifest's
  premise that this is real drag-source code, not test-only scaffolding.
- Re-read `Scripts/wayland-drive.sh`'s exported action functions
  (`ctl pointer_command move click type key shot title`, line 338 of the current file): `click`
  and `move` (lines ~272-273) each issue one atomic pointer op over the persistent virtual-pointer
  FIFO. There is no press-hold, motion-while-held, or release primitive — nothing composes a
  drag. `system.capabilities` (54 methods, confirmed live this session) has no drag-shaped
  control method either — every method there is click/select-shaped.
- `WAYLAND-LANE.md` states outright: "Pointer drags, right-click, modifiers/chords and
  IME/non-ASCII text are not yet exercised," and routes drag rows to `DISPLAY=:1` + the drive
  lock. This manifest and orchestrator instructions explicitly forbid `DISPLAY=:1` and
  `linux-drive.sh` for this slice.

Claim: `could-not-reach` — same conclusion as the E08 sweep's independent re-confirmation: the
gesture this row requires (press-hold-drag) has no primitive anywhere on the Wayland lane, and
the one route that has one is barred to this slice. This is an environmental block, not a
platform-impossible claim — the production drag/drop code itself reads as real on both ends.
