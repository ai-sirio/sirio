# Finish line — domain / settings / persistence, part 2 (lane wf-dom3)

Cites `docs/linux-rewrite/EVIDENCE-STANDARD.md`. Live drives on this host (x86, COSMIC/Wayland,
`WAYLAND_DISPLAY=wayland-1`), 2026-08-18, via `Scripts/wayland-drive.sh`. Binary pinned:
`cp rust/target/debug/tiller /tmp/wf-dom3-tiller && export TILLER_WL_BIN=/tmp/wf-dom3-tiller`.

## Rows in scope

Grep of `INVENTORY-LEDGER.md` for prefixes `F-CORE-DOM`, `F-SET`, `F-PER`, `F-WIN`, `F-CORE-USG`,
`F-CHG`, `F-TERM` that are not `PASSED` and not `N/A — platform`, as of HEAD before this pass:

| id | prior verdict |
|---|---|
| F-WIN-03 | half-proven |
| F-WIN-10 | half-proven |
| F-CHG-02 | FAILED — defective |
| F-CHG-15 | half-proven |
| F-CHG-18 | FAILED — defective |
| F-PER-01 | half-proven |
| F-PER-05 | NOT EXERCISED |
| F-PER-07 | NOT EXERCISED |
| F-SET-14 | half-proven |
| F-SET-15 | FAILED — absent |
| F-SET-18 | half-proven |
| F-SET-22 | half-proven |
| F-TERM-03 | half-proven |
| F-TERM-10 | half-proven |
| F-TERM-SCR-02 | half-proven |
| F-TERM-PTY-04 | half-proven |
| F-TERM-UI-02 | half-proven |
| F-CORE-DOM-02 | half-proven |
| F-CORE-DOM-03 | half-proven |
| F-CORE-DOM-05 | half-proven |
| F-CORE-DOM-07 | half-proven |
| F-CORE-DOM-08 | half-proven |
| F-CORE-USG-07 | half-proven |

Priority per brief: the five rows a predecessor left NOT EXERCISED (F-WIN-03, F-WIN-10, F-PER-01,
F-PER-05, F-PER-07) — driven first, below — then F-CORE-DOM-03/07 (currently carried on unit
tests only), then remaining half-proven rows as budget allows.

Status: IN PROGRESS — rows appended below as driven, each committed individually.

---
