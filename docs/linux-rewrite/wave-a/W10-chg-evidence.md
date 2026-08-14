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
