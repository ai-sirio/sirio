# H4-window verdicts

Critic pass, independent of the builder. Both rows read from source
(`crates/tiller_ui/src/titlebar.rs`, `crates/tiller/src/main.rs`), re-run per-crate
(`cargo test -p tiller_ui`, `cargo test -p tiller`), and driven live under `Scripts/wayland-drive.sh`
this pass — evidence below is my own, not a repetition of the builder's report.

## `F-WIN-09` — follow the system title-bar double-click preference — **half-proven**

Code reads correctly: `Titlebar`'s drag-area `div` has a real second `on_mouse_up` that checks
`event.click_count == 2` and dispatches to `on_maximize`/`on_minimize`/`show_window_menu` through the
same closures the traffic lights already use — not a mock, the production path. `cargo test -p
tiller_ui double_click` (the builder's own stated filter, `titlebar::double_click`, actually matches
**0** tests — the module path is `titlebar::tests::double_click_*`; a documentation bug, not a code
one) — correctly invoked, 6 tests pass; full crate 322 passed.

Two live checks beyond anything the builder's report claims:

1. **Discriminating live proof the gsetting read is real, not hardcoded.** Flipped the actual system
   value (`gsettings set … minimize`), re-ran `double_click_action_from_system_resolves_without_panicking`
   (which asserts `ToggleMaximize`) — it **failed**, proving `from_system()` genuinely reads live state
   dynamically rather than always returning the graceful default. Restored to `toggle-maximize`
   afterward; re-run passed. The builder's own report admitted this same test does not discriminate on
   its own (default equals this box's actual value) — this flip is the missing discriminating half.
2. **Live click_count==2 mechanism, proven via positive control.** Found via
   `docs/linux-rewrite/wave-d/D-MAIN-4-report.md` that two `click` calls land seconds apart
   (`verify_nested_sway`'s swaymsg round trip per call) — outside GPUI's 400ms
   `DOUBLE_CLICK_INTERVAL` — so 2 clicks never reach click_count 2; 3 rapid clicks do. Reproduced this
   myself: 3 rapid `click` calls on a Files-panel row opened `README.md` as a new tab
   (`/tmp/critic-h4win09d/03-after-triple-click-readme.png`), proving the real GPUI Wayland click_count
   pipeline delivers a genuine double-click in this exact harness. Then drove the identical 3-click
   cadence at the titlebar drag area with the live gsetting set to `minimize`
   (`/tmp/critic-h4win09e/03-after-triple-click-titlebar.png`) — **zero pixel diff and zero
   `swaymsg -t get_tree` diff** versus baseline, even though a real click_count==2 was delivered (per
   the positive control above). This independently confirms — via two instruments, not just
   screenshots — the harness ceiling H3-tray-report already found for `set_minimized`: this nested-sway
   config force-fullscreens the single toplevel regardless of the app's own WM requests, so no
   minimize/maximize/menu effect from *any* code path (new or pre-existing) can be observed here.

Verdict: the read-the-preference half and the click_count==2 gate are proven live, discriminatingly,
this pass. The row's actual stated success criterion ("window should minimize") remains neither
confirmed nor contradicted — categorically unobservable in this harness, not a defect. `half-proven`.

## `F-WIN-11` — the update toast's five user-visible states — **PASSED**

Drove the full sequence live in one `wayland-drive.sh` invocation with values that do not match the
builder's own report (`9.9.9-critic`, `42%`, `CRITIC_FAIL_TEST` / `CRITIC_FAIL_TEST2`) so the capture
could only have come from this drive, not a stale frame:

- `update.event event=available version=9.9.9-critic` → toast "Tiller 9.9.9-critic is available"
  (`/tmp/critic-h4win11/03-available.png`)
- `download-progress percent=42` → "Downloading Tiller… 42%" with a ~42%-filled bar
  (`04-downloading-42.png`)
- `install-started` → "Installing update…" (`05-installing.png`)
- `finished` → "Tiller is up to date" (`06-uptodate.png`)
- `failed message=CRITIC_FAIL_TEST` → "Update Failed: CRITIC_FAIL_TEST" + Retry (`07-failed.png`)
- Real click on the drawn `update-toast-dismiss` `×` (`/tmp/critic-h4win11b`, coordinates re-found
  after a first miss) removed the toast entirely — capture reverts to the exact baseline colour count
  (6777), confirming state truly returned to `Idle`, not just visually similar
  (`/tmp/critic-h4win11b/03-after-dismiss-click.png`).

`cargo test -p tiller update_event` and `drawn_update_toast` both pass in isolation. This is the real
control-socket → real `UpdateState::transition` → real render → real click path, end to end, with
discriminating values I chose myself. `PASSED`.
