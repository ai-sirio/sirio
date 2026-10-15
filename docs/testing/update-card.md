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

## The a11y defect and its guard

A debug build panicked with `Duplicate a11y node id` when the card drew the agent
warning and an error together: `message()` gave both one id, and bezel's
accessibility tree rejects the second node. Fixed in 20530336 (ids differ among
messages drawn together). The guard is stage ui's `ctl ping` after each error
capture (the failed and refused cards): a panic ends the process, so the next
ping has no one to answer.

Limitation: the guard only means something while bezel's accessibility adapter
(accesskit_unix) is active, and this run does not prove that it is. The adapter
connects to the AT-SPI bus when `org.a11y.Status.ScreenReaderEnabled` changes to
true. In three probes on the run's private buses, a change of that flag (false,
then true) opened no new connection from the app, so the guard can pass without
the adapter. The run gives the app a private session bus and AT-SPI bus, with the
accessibility flags (`IsEnabled`, `ScreenReaderEnabled`) turned on there before the
app starts (under `env -i` they are off), and notes whether the app holds a
connection on that AT-SPI bus (a read-only `busctl list`; a NOTE line in the
transcript).
