# Drive slice E09-prj+edit — 6 rows

Families: F-PRJ+F-EDIT.

Exercise each row live and record what you saw. **Do not edit the ledger**;
a different agent adjudicates. For a `half-proven` row the evidence column
names the half already proven — drive only the missing half and say which.

| row | ledger line | current verdict | evidence recorded so far |
|---|---|---|---|
| `F-PRJ-06` | 99 | half-proven | **empty-URL disablement proven live**: with the URL field empty the `Clone repository` button renders dimmed (`orch15-clone-b.png`) and after typing a valid URL it renders enabled (`orch16-url-b.png`). **The double-submission guard is not exercised** — that needs two rapid clicks on the enabled button and a real clone in flight, which this drive deliberately did not start |
| `F-PRJ-09` | 102 | half-proven | **empty-name disablement proven live**: with the name field empty the `Create project` button renders dimmed (`orch17-create-b.png`). **The duplicate-submission guard is not exercised** — that needs a real create in flight, which this drive deliberately did not start |
| `F-PRJ-14` | 107 | NOT EXERCISED | **pass 13 superseded** — an `Avatar` tab is present in the mounted picker beside `Icon` and `Emoji` (`orch17-settings-b.png`). Its contents were not opened, so nothing is claimed about the PNG/GitHub/favicon fields. Note the same unwired `on_change` that defeats `F-PRJ-13` would defeat this too |
| `F-PRJ-16` | 109 | NOT EXERCISED | **pass 13 superseded** — an `Emoji` tab is present in the mounted picker (`orch17-settings-b.png`). The tab was not opened, so the single-emoji entry itself is unexercised. The unwired `on_change` behind `F-PRJ-13` applies here too |
| `F-EDIT-05` | 223 | NOT EXERCISED | P101 critic started from `cc73d71` without adopting the pass-18 drive. The final P95 binary did launch on `:1` and a fresh Files context-menu frame was captured (`/tmp/crit17/b2-notes-menu.png`), but no file-editor conflict gesture ran: the next XTEST drive was blocked by the shared live holder. `TILLER_DRIVE_LOCK_WAIT=900 Scripts/linux-drive.sh …` expired with `pid=657280 l… |
| `F-EDIT-12` | 230 | NOT EXERCISED | P101 headless lane did open Changes and, on the required second read, returned the real non-binary `notes.md` diff (+2) from `/tmp/p101-fixture`; current production `changes.rs` also has a diff-row `on_drag`. That is not the required drag gesture or pane receipt. The `:1` XTEST drive needed to drag the row was blocked for 900 s by the live `sonnet` lock holder (`pid=657280`)… |
