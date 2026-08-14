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
