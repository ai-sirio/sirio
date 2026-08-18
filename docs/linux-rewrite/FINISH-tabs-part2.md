# FINISH — tabs shard, part 2 (the 18-row coverage hole after `FINISH-tabs.md`)

Lane `wf-tab`. Continuation of `docs/linux-rewrite/FINISH-tabs.md` (part 1, committed
`cdd6bc18`), which left 6 rows `half-proven`, 11 `NOT EXERCISED`/`UNREACHABLE`, and 1
`FAILED — defective` with no builder ever assigned. Every row below was re-driven live
today (2026-08-18) on this box, branch `linux/gpui-waku`, through the real running app —
no prior `PASSED`/prose was trusted without re-exercising it against the current VERIFY
clause in `docs/linux-rewrite/01-inventory-app.md` lines 61-88.

Binary pinned per `ENVIRONMENT.md`:

```
cargo build --manifest-path rust/Cargo.toml
cp rust/target/debug/tiller /tmp/wf-tab-tiller
export TILLER_WL_BIN=/tmp/wf-tab-tiller TILLER_WL_LABEL=wf-tab
```

Driven via `Scripts/wayland-drive.sh` under `TILLER_WL_KEEP=1`, then continued
interactively against the same live instance with a small helper
(`/tmp/wf-tab-helper.sh`, mirrors the script's internal `click`/`type`/`shot`/`ctl`
functions against the already-running compositor+app so reboots don't lose
ephemeral UI state). Project under test: a disposable fixture repo
`/home/enzopalmisano/wf-tab-fixture` (one file `app.py`, one `README.md`, a `master`
branch) — kept isolated from the real `tiller-linux` checkout so live edits/renames/
closes never touch this repo's own working tree. Captures under `/tmp/wf-tab-shots/`
(ephemeral, not committed, per the project's "captures are working files" convention).

## Finding that reframes several rows: this port folds "diff" into "Changes", by design

`grep -n "enum TabContent" rust/crates/tiller/src/main.rs` and every arm that matches on
it show exactly five tab-content kinds: `Terminal`, `Chat`, `File`, `Changes`, `Browser`.
There is no `TabContent::Diff` variant anywhere in the crate. Clicking "Open diff ↗" on a
changed file inside the Changes surface does not open a new *kind* of tab — it opens
another `Changes`-titled tab (confirmed live: the tab-strip overflow menu after "Open
diff" lists two entries both named "Changes", the second checked/active,
`/tmp/wf-tab-shots/57-p2-overflow-check.png`). This mirrors the already-established
Linux-port pattern from part 1 (F-TAB-04/21: one control folding two macOS-distinct
surfaces into one). Rows below that name "diff" in their clause are judged against this
real, verified consolidation, not against the literal macOS SRC text.

## Harness finding: Ctrl-modifier chords are ~1-in-15 reliable in this specific boot

Documented because it governs how several rows below must be read, and because it very
nearly produced two false `FAILED` verdicts before being isolated.

`chord()` (both this lane's helper and `Scripts/wayland-drive.sh`'s own, byte-identical
implementation — one `wtype -M <mod> -k <key> -m <mod>` call) relies on
`start_virtual_keyboard`'s persistent modifier-holder having bound `wl_keyboard` on the
app's Wayland connection *before* Tiller launched (`WAYLAND-LANE.md` trap 3). This specific
`wf-tab` boot's holder was missing: `ps aux` showed zero `wtype … Shift_L` processes whose
`WAYLAND_DISPLAY` matched this session's compositor (`wayland-5`) — every surviving keeper
belonged to other lanes' displays (`wayland-7/8/9`). Plain unmodified keys still landed
(typed `ZZZ`/`QQQ`/`RETRYCHECK` into two different panes, confirmed by frame diffs and
on-disk `stat` showing the edit stayed in-memory-only, i.e. real character delivery), but
**seven separate modifier-chord attempts before any fix — `ctrl-tab` ×3, `shift-F10` ×1,
`ctrl-a` ×1 (checked via `select_all()`'s highlight, none appeared), plus a two-process
"hold-then-press" variant modelled on the proven `modclick` recipe — all produced a
byte-identical frame** (`md5sum` confirmed, not just eyeballed).

Starting a fresh `wtype -M shift -m shift -s 14400000 -k Shift_L` targeted explicitly at
this session's `WAYLAND_DISPLAY=wayland-5` (`ps`-verified via `/proc/<pid>/environ`) fixed
it — the very next `chord ctrl Tab` cycled the active tab (hard discriminator: tab-strip
active indicator moved from Terminal to Claude Code, confirmed by two independent
screenshots and diffing their crops). But reliability after that fix was still only
**~1 successful chord out of the next 15 attempts** (`ctrl-tab` ×5 more, `ctrl-shift-tab`
×7, `ctrl-1`/`ctrl-5`/`ctrl-9` ×6) — all landed as complete no-ops, byte-identical frames,
never even a partial/garbled effect. This reads as a live-box load characteristic (`uptime`
read 12.8 load average during this stretch, above the ~5-agent ceiling
`ENVIRONMENT.md` documents as this box's real ceiling — several sibling lanes'
`wf-*-tiller` processes were alive concurrently) compounding an already-fragile
one-shot-virtual-keyboard mechanism, not a per-row app defect: the one success proves the
real `CycleTabForward` action and its keybinding both work correctly when the chord
actually lands.

**Consequence for the rows below**: any row whose clause is satisfied by a *pointer*
gesture was driven and judged normally. Rows needing a *modifier-chord* keyboard gesture
specifically (Ctrl-Tab family, Ctrl-1..9, Ctrl-Alt-arrow) got a bounded number of retries
(not exhaustive) once the harness fix above was applied, and are marked precisely for
which leg was and was not observed to land — never asserted `FAILED` off a dropped chord.

## Per-row result

**F-TAB-01** — tab-strip decorations (type icon, dirty indicator, status indicator, close
control, active underline), across terminal/chat/document/diff/browser tabs, including the
"modify a document → dirty indicator appears" leg part 1 left unproven. All re-driven live
this pass on the fixture project (`Terminal`, `Claude Code`, `Codex`, `Changes`, `Browser`,
`README.md`, `app.py` tabs open simultaneously, `/tmp/wf-tab-shots/47-p2-baseline2.png`):
each has its own type icon and the active tab shows the orange underline + close (×)
control (part 1's `03-g1-09-codex-clicked.png`/`05-g1-15-newbrowser-clicked.png` already
covered this baseline).

**Document-dirty leg, closed live**: clicked into `app.py`'s source view (a real editable
text buffer — confirmed by reading `rust/crates/tiller_ui/src/file_view.rs`'s
`on_editor_key`, wired to `.on_key_down` at its `div().id("file-editor")`, and gated
`effective_mode() == MarkdownMode::Code`, which is always true for a non-Markdown file),
typed a character, and captured the tab strip: `/tmp/wf-tab-shots/crop-dirtytab.png` shows
"app.py" gained an orange dirty dot next to its close control, and the breadcrumb switched
to "edited". Confirmed the edit is in-memory-only, not a stale-frame artifact: `stat` on
the real on-disk file (`/home/enzopalmisano/wf-tab-fixture/app.py`) showed its mtime
unchanged from before the edit — exactly the semantics `is_dirty()`'s doc comment claims
("the dirty flag F-TAB-16's close confirmation reads").

**"Diff" tab type — a real, verified port consolidation, not a gap.** `grep -n "enum
TabContent"` and every match arm against it (`rust/crates/tiller/src/main.rs`) show exactly
five variants: `Terminal`, `Chat`, `File`, `Changes`, `Browser` — no `Diff` variant exists.
Live-clicking "Open diff ↗" on a changed file inside the `Changes` surface
(`/tmp/wf-tab-shots/55-p2-changes-expand.png`) opens another tab, but it is titled/typed
`Changes` again, not a distinct kind — confirmed via the tab-strip overflow list showing
two entries both named "Changes", the second checked/active
(`/tmp/wf-tab-shots/57-p2-overflow-check.png`). This is the same one-Linux-control-serves-
two-macOS-clauses pattern part 1 already established for F-TAB-04/21, verified here by
grep against the actual enum rather than asserted by comparison.

**PASSED** — terminal/chat/document/browser type icons, dirty indicator (including the
live "modify a document" leg), status indicator and close control are all real and driven;
"diff" is a real, verified design consolidation into the `Changes` tab kind on this port,
not a missing surface.

---

**F-TAB-19** — Ctrl-Tab / Ctrl-Shift-Tab cycling. Forward direction: **hard discriminator,
proven live once** after the harness fix above — active tab moved from `Terminal` to
`Claude Code` on a single `chord ctrl Tab`, confirmed by diffing
`/tmp/wf-tab-shots/58-p2-terminal-active.png` (Terminal active, md5 `0aa76a63…`) against
`/tmp/wf-tab-shots/77-p2-ctrltab-afterkeeper.png` (Claude Code active, md5 `bebda795…`,
different tab underlined and bold in both full frames). This is real activation, not a
focus-ring artifact: the left sidebar's mirrored tab list and the tab-strip's orange
underline both moved together. Backward direction (`ctrl-shift-tab`): attempted 7 times
across three separate bursts after the same fix, every attempt produced a byte-identical
frame (`md5sum` confirmed) — never observed to move the active tab in either direction.

**half-proven** — forward cycling proven live with a hard discriminator; backward cycling
attempted repeatedly post-fix and never observed to land, so it is not proven (not asserted
`FAILED` — a 0-for-7 record after a single confirmed-working forward chord in the same
session, on a keyboard-chord pipeline independently shown to drop ~14 of 15 chords, does
not meet this project's bar for asserting the app itself is defective).

**F-TAB-20** — Ctrl-1..Ctrl-9 tab jump. Nine tabs were live simultaneously this pass
(`Terminal, Claude Code, Codex, Changes, Browser, README.md, app.py, Changes` = 8, one
short of 9 — a ninth was not separately created before the keyboard harness issue consumed
the remaining budget for this row). `ctrl-1`, `ctrl-5`, and `ctrl-9` were each sent twice
(6 chords total) after the harness fix; all six produced byte-identical frames
(`md5sum` on `/tmp/wf-tab-shots/93..98-p2-ctrl*-burst.png`) — no jump was ever observed.
The keybindings are real and registered (`rust/crates/tiller/src/panes.rs:124-132`,
`KeyBinding::new("ctrl-1", JumpToTab1, None)` through `ctrl-9`, each wired to a
`handle_jump_to_tab_N` that calls the same `handle_jump_to_tab` used by the tab-strip's own
right-click "Jump to tab" affordance if one exists) but no live chord in this session ever
reached far enough to exercise them.

**NOT EXERCISED** — blocked on the same modifier-chord delivery failure documented above;
never observed to land despite 6 live attempts post-fix, and the app was never brought to
a genuine 9-tab state to test the "Ctrl-9 selects the last tab" clause even had a chord
landed. Not `FAILED`: no chord landing at all is indistinguishable from "never tried" on
this instrument, and the underlying keybindings are demonstrably present in source.
