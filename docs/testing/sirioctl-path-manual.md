# sirioctl on the user's PATH — manual rows

`Scripts/Tests/test-sirioctl-e2e.sh` proves the stable copy of `sirioctl`, its
PATH link, the pane environment and the hooks in an isolated `HOME`, against a
debug build. On macOS it runs state-only in `macos-check.yml`, and it does not
check the macOS link at all, because macOS places the link where its login PATH
searches. These rows are what it cannot run: the administrator prompt of
*Install in PATH*, the Windows user PATH and its removal by the Inno uninstaller,
a real AppImage, a real login shell, and a desktop launch. They are the evidence
for §5.2 and §5.4 of `docs/superpowers/specs/2026-10-17-ade-programme-design.md`,
which names the administrator prompt as a manual row recorded with the date it
was done.

## Who runs a row, and when

Each row is run by hand, on the platform it names, against the build its Build
column names. When it is done, the *Done on* cell gets the date and the platform
version. No script runs these rows and no gate records them. A row left empty
has not been run, so the release is not verified on that platform until it is
filled in.

## The rows

| Platform | Build | Row | Expected | Done on |
|---|---|---|---|---|
| macOS 15 | Nightly DMG, `Sirio.app` in `/Applications`; `/usr/local/bin/sirioctl` absent or already Sirio's | Settings → General → *Install in PATH* | Exactly one administrator password prompt. The card reads `On PATH at /usr/local/bin/sirioctl`. `/usr/local/bin/sirioctl` links to the stable copy. In a new Terminal, `sirioctl version` prints the app's version | |
| macOS 15 | same, with `/usr/local/bin` writable by the user (`test -w /usr/local/bin`) | Settings → General → *Install in PATH* | No prompt at all. The card reads `On PATH at /usr/local/bin/sirioctl`, and `sirioctl version` works in a new Terminal | |
| macOS 15 | same as the first row, with `/usr/local/bin` not writable | *Install in PATH*, then Cancel at the prompt | The card says the link was not made (`Could not link: the administrator link was not made: …`, osascript's reason). `/usr/local/bin/sirioctl` does not exist afterwards | |
| macOS 15 | the notarized DMG's `Sirio.app` | `codesign --verify --strict --verbose=2 Sirio.app/Contents/MacOS/sirioctl` | Exit 0: the binary is signed with the hardened runtime, as `sirio-host` is | |
| Windows 11 | the installed Inno build of the release | First launch of the installed app, then a **new** PowerShell: `sirioctl version` | `sirioctl` is found, in `%LOCALAPPDATA%\Sirio\bin`, which is on the user's `Path` (HKCU). `sirioctl version` prints the app's version | |
| Windows 11 | same install, after *Add or remove programs* → Sirio → uninstall | A **new** PowerShell: `Get-Command sirioctl` | Not found. The Inno uninstaller removed the `Path` entry | |
| Linux, a real AppImage | AppImage installed with `install.sh` (not the E2E's stand-in) | First launch of the installed AppImage. Then a tab opened in Sirio: `sirioctl ping` | `~/.local/bin/sirioctl` links to the stable copy at `<XDG_DATA_HOME or ~/.local/share>/Sirio/bin/sirioctl`. The tab prints `pong`. If the card says the login shell does not search `~/.local/bin`, record that as the result | |

## Procedures

**macOS, Install in PATH.** Before the first row, check that
`/usr/local/bin/sirioctl` does not exist, or is already a link to the stable copy.
For the no-prompt row, `test -w /usr/local/bin` must succeed; for the prompt rows
it must fail. Open Settings → General and press *Install in PATH*. Enter the
administrator password once, then run `sirioctl version` in a new Terminal
window. The stable copy is `~/.local/share/Sirio/bin/sirioctl`, or
`$XDG_DATA_HOME/Sirio/bin/sirioctl` when that variable is absolute. For the cancel
row, press Cancel at the prompt, then check the card and `ls -l
/usr/local/bin/sirioctl`.

**macOS, signed bundle.** Mount the notarized DMG, copy `Sirio.app` out of it,
and run the `codesign` line on `Contents/MacOS/sirioctl` inside that copy.

**Windows, PATH after install.** Launch the installed app once; the startup step
adds `%LOCALAPPDATA%\Sirio\bin` to the user's `Path`. Open a new PowerShell (an
existing one keeps the old `Path`) and run `sirioctl version`. After uninstalling,
open another new PowerShell and run `Get-Command sirioctl`.

**Linux, a real AppImage.** Install the AppImage with `install.sh`, launch it
from the installed location, and quit it. Check `ls -l ~/.local/bin/sirioctl`,
which should name the stable copy. Open a tab in Sirio and run `sirioctl ping`.
Record the login shell (`echo $SHELL`) and whether `~/.local/bin` is on its PATH.
