# F-CORE-UI — fresh critic pass (live drive + source audit)

Section: **F-CORE-UI** (2 rows, Package tier / `TillerCore` — small UI-adjacent domain helpers:
appearance-mode resolution and the native-updater state machine).

Contract source: `docs/linux-rewrite/02-inventory-packages.md` lines 70–71.

- `F-CORE-UI-01` — "Appearance mode supports system, light, and dark choices and maps them to the
  corresponding native appearance... VERIFY: Select each appearance mode, restart, and inspect
  effective theme selection and system-following behavior."
- `F-CORE-UI-02` — "Update state models checking, available, downloading, installing, up-to-date,
  failed, and idle transitions for the native updater... VERIFY: Feed update callbacks for each
  state and inspect the user-visible update state and failure handling."

The existing ledger rows (`INVENTORY-LEDGER.md:410-411`) were both `PASSED` on the strength of a
`cargo test` run alone ("executed not read"). Per this task's brief I re-judged both by driving a
live app, and did not echo the prior verdict.

**Environment**: `/dev/shm/tt/debug/tiller` + `tillerctl`, driven via `Scripts/wayland-drive.sh`.
Labels `sweep20ui*` (five separate invocations: `sweep20ui`, `sweep20uifull`, `sweep20uiconfirm`,
`sweep20uireproduce`, `sweep20uiupdate`, `sweep20uipersist`), outdir `/dev/shm/sweep-20-F-CORE-UI`
(40 screenshots), throwaway fixture repos under `/dev/shm/sweep20ui*-fixture`. All cleaned up after
the pass; nothing left outside `/dev/shm`.

## Row verdicts

| row id | verdict | evidence |
|---|---|---|
| `F-CORE-UI-01` | PASSED | Live-driven through the real Settings > Appearance UI, not a unit test. `ctl surface.settings.open section=appearance` then `click` on each of the three segmented-control labels; each click was independently confirmed two ways: (a) `ctl surface.settings.read` reporting `"theme":"system"` → `"light"` → `"dark"` → `"system"` in sequence, and (b) the segmented control's own selected-pill highlight moving to match (screenshots `04-42-repro-dark.png` clearly shows "Dark" bold/pilled while "System"/"Light" are muted; `05-43-repro-system2.png` shows the pill back on "System"). **Restart clause covered live**: set theme to Dark (`click 1275 141`), let the app process exit at the end of one `wayland-drive.sh` invocation, then launched a **second, independent process** against the same on-disk `TILLER_DB` (`/tmp/sweep20uipersist.sqlite`) with the same label — `ctl surface.settings.read` on the fresh process reported `"theme":"dark"` with **zero interaction**, and `03-62-persist-restart-appearance.png` shows the "Dark" pill pre-selected on first paint. This is a genuine cold-start reload of `AppSettings` from SQLite, not an in-memory carry-over. |
| `F-CORE-UI-02` | PASSED | Live-driven through the real control-socket entry point `ctl update.event event=...` (the same `tillerctl update.event` surface a real updater backend would call) with the toast visible on the **main workspace view** (Settings must be closed — see harness note below). Drove and screenshotted all 7 states in one continuous run: `check-started` → "Checking for updates…" (`03-51-update-checking.png`), `available version=1.4.0-critic` → "Tiller 1.4.0-critic is available" + Download action (`04-52-update-available.png`), `download-progress percent=150` → "Downloading Tiller… 100%" with a **full-width** progress bar, visually confirming the 150→100 clamp (`05-53-update-download-clamped.png`), `install-started` → "Installing update…" (`06-54-update-installing.png`), `finished` → "Tiller is up to date" in a done/green tint (`07-55-update-uptodate.png`), `failed message=CRITIC_NETWORK_MARK_UI20` → "Update failed: CRITIC_NETWORK_MARK_UI20" in red with a Retry action, the literal marker string round-tripping through the control socket into the render (`08-56-update-failed.png`), `reset` → toast fully gone (`09-57-update-reset.png`). Every user-visible state and the failure message are covered. |

## What I disagree with in the prior record

The ledger's `F-CORE-UI-01` evidence was `cargo test -p tiller_project ui:: ... appearance_follows_system_only_in_system_mode ... ok`, i.e. the unit test on `tiller_project::ui::AppearanceMode::resolve`. **That type is dead code as far as the live app is concerned.** I grepped the whole tree:

```
grep -rn "AppearanceMode" --include=*.rs . | grep -v tiller_project/src/ui.rs | grep -v tiller_persistence | grep -v tiller/src/main.rs | grep -v tiller/src/session.rs
# only hit: rust/crates/tiller_project/src/lib.rs:74:pub use ui::{AppearanceMode, UpdateEvent, UpdateState};
```

`tiller_project::ui::AppearanceMode` is defined, unit-tested, and re-exported from the crate root —
and then imported by **nothing**, anywhere, ever. The type that actually drives the live Settings >
Appearance screen is a completely different pair of types: `tiller_persistence::AppearanceMode`
(the persisted user choice) and `tiller_theme::ThemeMode`/`Appearance` (the resolution logic against
`cx.window_appearance()`/the XDG portal, in `rust/crates/tiller_theme/src/lib.rs`), which main.rs
imports and maps back and forth (`main.rs:11695-11734`). The prior PASSED's *reasoning* was
therefore unsound — it tested an orphaned duplicate, not the feature — even though, per my own live
drive above, the underlying capability genuinely does work. I'm flagging this loudly per the task
brief; the verdict itself stands as PASSED on my own live evidence, not on the cited test.

By contrast `F-CORE-UI-02`'s cited test (`updater_reaches_every_user_visible_state_and_clamps_progress`
on `tiller_project::ui::UpdateState`/`UpdateEvent`) genuinely *is* the same type main.rs uses for the
live update toast (`ControlAction::UpdateEvent`, `Workspace.update_state`, `render_update_toast`) —
confirmed by grep and independently re-proven live above. No complaint there; the prior evidence and
the live mechanism are the same code path.

## A harness trap worth recording (not an app defect)

`render_update_toast` is only mounted in the **main workspace render branch**; when `show_settings`
is true, `render()` early-returns a completely different tree (`main.rs:10787-10805`) that has no
toast child at all. Firing `ctl update.event` while Settings is open silently queues the state
change (confirmed via `"queued":"true"`) but nothing is visible until Settings is closed — this is
correct, deliberate layering (Settings is a full-screen overlay), not a bug. My first attempt made
this mistake (screenshots `06-20-update-checking.png` through `09-23-update-installing.png` in the
combined `sweep20uifull` run show the Appearance screen with no toast); I re-drove F-CORE-UI-02
cleanly in `sweep20uiupdate` with Settings closed, which is the evidence cited in the table above.

## An anomalous, non-reproducing frame — reported for visibility, not counted as a defect

In the very first combined run (`sweep20uifull`, label `sweep20uifull`), clicking "Light"
(`click 1218 141` right after opening Settings > Appearance) produced a frame
(`03-11-appearance-light-clicked.png`) where `ctl surface.settings.read` correctly reported
`"theme":"light"`, but the segmented control's pill was **still** visually on "System" and the
background colours never left the dark palette — i.e. state and render disagreed. I treated this as
a possible real defect and tried hard to reproduce it:

- An isolated fresh-boot run (`sweep20uiconfirm`) clicking "Light" as the very first and only
  action: pill and colours both switched to light correctly, immediately and after a further 3s and
  6s settle (`31-fresh-light-3s.png`, `32-fresh-light-6s.png`).
- A full re-run of the exact original System→Light→Dark→System sequence (`sweep20uireproduce`,
  fresh label): every transition rendered correctly this time, pill included
  (`41-repro-light.png` shows "Light" correctly bold/pilled).

Two clean reproductions against one anomalous frame, on a host this task's own briefing describes as
running many concurrent critic instances at once (`pgrep`/`ps` at the time showed multiple other
`sweep-*` labels active). Per the standard of proof, I'm not promoting a single non-reproducing
frame to a FAILED verdict — but recording it here in case another pass on a quieter host sees it
again.

## Unreachable / not exercised

None. Both rows were fully driven live, including the restart clause of `F-CORE-UI-01` and every
named state of `F-CORE-UI-02`.
