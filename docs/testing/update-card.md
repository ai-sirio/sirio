# The update card — manual rows

`Scripts/Tests/test-update-ui-e2e.sh` proves the card and the Linux relaunch.
These rows are what it cannot run: the relaunch on macOS (`open` of the
swapped bundle) and on Windows (Inno closes and relaunches Sirio; the session
is saved before the installer starts). Run each against a Nightly installed
from the release job, with a newer Nightly published.

| Platform | Build | Steps | Expected | Result | Date |
|---|---|---|---|---|---|
| macOS 15 | Nightly DMG in /Applications | Launch; card shows; *Restart to update* | Sirio quits, reopens on the new version; Settings → General shows it; projects and tabs restored | | |
| macOS 15 | same, an agent running | *Install without restarting*, then *Restart now* | Card warns first; after install says *installed — restart to use it*; Restart reopens on the new version | | |
| Windows 11 | Nightly installer | Launch; card shows; *Update and restart* | Inno closes and reopens Sirio on the new version; projects and tabs restored | | |
| Linux, a real AppImage | AppImage installed with `install.sh` (not the E2E's stand-in) | Launch the installed AppImage; card shows; *Restart to update* | Sirio quits and relaunches from the swapped AppImage, and Settings → General shows the new version; projects and tabs restored. Risk: gpui's restart script inherits the old process's working directory and environment, which may lie inside the old FUSE mount of the AppImage. Record whether the relaunch runs from the new file and whether the old mount is still in use | | |

**Windows relaunch.** Sirio relies on Inno's Restart Manager to reopen it after
*Update and restart*. Restart Manager restarts only applications that called
`RegisterApplicationRestart`, and Sirio does not call it. The Windows row above
decides this: if Sirio is not reopened, Windows needs `RegisterApplicationRestart`
(or an equivalent) before release. That is a fix in the Windows installer or
host code, not in the card. Until the row is recorded, the Windows release is
not verified for the relaunch.

## The a11y defect and its guard

A debug build panicked with `Duplicate a11y node id` when the card drew the agent
warning and an error together: `message()` gave both one id, and bezel's
accessibility tree rejects the second node. Fixed in 20530336 (ids differ among
messages drawn together). The guard is stage ui's `ctl ping` after each error
capture (the failed and refused cards): a panic ends the process, so the next
ping has no one to answer.

The guard means something only while bezel's accessibility adapter (accesskit_unix)
is active. Stage ui now asserts it: the debug-only `surface update read` reports
`a11yActive` from bezel's `Window::is_a11y_active()`, and the run waits for
`a11yActive true` before the failed and refused captures. A run whose adapter is
not active fails there rather than passing the guard vacuously. The run gives the
app a private session bus and AT-SPI bus, with the accessibility flags
(`IsEnabled`, `ScreenReaderEnabled`) turned on there before the app starts (under
`env -i` they are off). A NOTE line in the transcript records how many connections
of Sirio the AT-SPI bus holds (a read-only `busctl list`).

Result of that proof (final runs, 2026-10-15, `final6-ui-light` and
`final6-ui-dark` under `~/.cache/st/update-ui-e2e/`): `a11yActive` was `true` in
all 27 card reads of each run, the assertion passed before the error captures in
both, and the AT-SPI bus held one connection from Sirio in both. The earlier
caveat (no new connection after a flag change, in three probes) is superseded:
the adapter is active from the first read, with the flags set before launch.
