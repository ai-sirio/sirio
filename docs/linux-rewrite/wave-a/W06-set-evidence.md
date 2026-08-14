# Wave A slice W06-set — evidence

## `F-SET-19`

Drove the missing half: **System-follows-real-desktop**, not just System-vs-explicit-Light click.

The app's Linux theme portal read is the real host `xdg-desktop-portal` (confirmed:
`org.freedesktop.portal.Settings.ReadOne org.freedesktop.appearance color-scheme` answers from the
live host session, not anything scoped to the nested Wayland compositor). On this host the active
portal backend is `xdg-desktop-portal-cosmic`, which reads COSMIC's own theme-mode file
(`~/.config/cosmic/com.system76.CosmicTheme.Mode/v1/is_dark`), not GNOME's `gsettings`/`dconf`
`color-scheme` key — flipping `gsettings set org.gnome.desktop.interface color-scheme default` alone
did **not** move the portal's answer (stayed `color-scheme=1`/prefer-dark); flipping the COSMIC
`is_dark` file did (portal answered `2`/prefer-light within ~2s, no daemon restart needed).

Drive, each launch fresh (theme mode defaults to `system`, confirmed via `ctl surface.settings.select
section=appearance` returning `"theme":"system"` every time — set_mode(System) re-triggers
`follow_portal` on each App::init, matching the "one-shot per selection" model):

1. `is_dark=true` (host's real original state) → launched app → Appearance capture
   `02-01-appearance-light-desktop.png` (mislabeled by me before I'd confirmed the propagation path;
   desktop was actually still dark at this point) → whole-frame grayscale mean **32.6/255** (dark).
2. Set `is_dark=false`, confirmed portal `ReadOne` → `2` → fresh launch → Appearance capture
   `02-03-appearance-system-desktoplight.png` → whole-frame mean **243.8/255**, matching the known
   light-theme baseline (`2026-08-14-light-settings-appearance.png` = 243.9/255) almost exactly.
3. Set `is_dark=true`, confirmed portal `ReadOne` → `1` → fresh launch → Appearance capture
   `02-05-appearance-system-desktopdark.png` → whole-frame mean **33.1/255**, matching the dark
   baseline (`2026-08-13-cosmic02-dark.png` = 26.7/255) closely and clearly distinct from the light
   reading.
4. Restored the host's real desktop to its original state before finishing: `is_dark` back to `true`
   (its starting value, saved to `/tmp/wavea-w06-is_dark.bak` before the first edit) and `gsettings
   color-scheme` back to `prefer-dark` (its starting value).

This is discriminating evidence the earlier click-driven half could not produce: the same theme mode
(`System`, freshly resolved) renders two clearly different, correctly-matched appearances depending
solely on the real desktop's live setting at launch time. Both halves of the row are now driven:
System-vs-explicit-Light (prior evidence, `02-16-fresh3.png` / `03-17-fresh3-after-click.png`) and
System-follows-desktop (this pass). The "one-shot per selection, not a live D-Bus subscription" caveat
from triage stands as recorded — confirmed again here since each resolve required a fresh
`select`/relaunch of System rather than a live update while already selected, though a live-signal
subscription itself (does the app react to a `SettingChanged` signal while already showing System,
with no re-click) was not separately isolated this pass and would be a small additional check, not a
different verdict.

**Captures:** `reference/linux-progress/wavea-W06-set/02-01-appearance-light-desktop.png`,
`02-03-appearance-system-desktoplight.png`, `02-05-appearance-system-desktopdark.png`.

**Claim:** exercised-working — System theme mode resolves to the real desktop's live light/dark
state on (re)selection/launch, verified both directions with a positive-control brightness
measurement against known baselines.

## `F-SET-24`

Not reached from this lane. Per `WAYLAND-LANE.md` ("The embedded browser does not work on this
lane" / "No webview content") and the row's own triage note, the browser's webview content requires
an X11 window handle; under this nested-Wayland instance only the browser chrome renders (address
bar, back/forward/stop, title readout) and the content area shows the documented
`Direct XCB build failed … Wayland(WaylandWindowHandle …)` error. `request_permission` (browser.rs:588)
has no caller besides real page content, so a permission-request/grant/revoke round trip cannot be
driven without real page content, which this lane cannot render.

This row is explicitly routed to the X11 (`DISPLAY=:1`) lane by `WAYLAND-LANE.md`, and this slice's
instructions bar using that lane (`Scripts/linux-drive.sh` / `DISPLAY=:1` are off-limits here — single
global mutex shared with other slices). The row's on-record evidence already establishes the empty
Permissions-card state as genuine (`04-03-settings-permissions.png`); the grant/revoke round trip
remains `could-not-reach` from the Wayland lane specifically, not from the app.

**Claim:** could-not-reach — grant/revoke needs real webview page content, which requires the X11
lane (out of scope for this Wayland-only slice); the empty-state half was already genuinely proven on
record.
