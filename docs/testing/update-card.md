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
