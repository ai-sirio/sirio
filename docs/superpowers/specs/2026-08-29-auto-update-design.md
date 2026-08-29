# Auto-update across Windows, Linux and macOS

Date: 2026-08-29
Status: approved, not yet implemented

## What this is

The specification the [auto-update map](https://github.com/ai-sirio/sirio/issues/12)
was chartered to produce. It answers three questions end to end: what a Sirio
release artifact is on each platform, where it is published and how it is
signed, and how a running instance discovers, downloads and applies an update.

Every decision here was taken in its own ticket and is linked from the section
that states it. This document is the assembled result, not a new argument — read
a ticket when you need the reasoning behind a line, not to find out what was
decided.

**It does not build the updater.** That is the effort this hands off to.

### Naming

The tickets were written before the Tiller → Sirio rename and say `Tiller`
throughout. This document uses the current names: the app is `Sirio`, the
binaries are `sirio` and `sirioctl`, the repository is `ai-sirio/sirio`.

### Relationship to the macOS release-platform design

[`2026-08-29-macos-release-platform-design.md`](./2026-08-29-macos-release-platform-design.md)
covers how macOS becomes the reference release platform and what the release
workflow's job graph looks like. It overlaps this document on the macOS
artifact and is the authority on the workflow's shape. Where the two disagree on
a literal string, **this document wins**: it carries the decisions taken after
it, and the identifier it records is one of them.

## Decisions at a glance

| Question | Decision | Ticket |
|---|---|---|
| Target triples | Three, one per OS. No Intel Mac, no ARM anything. | [#16](https://github.com/ai-sirio/sirio/issues/16) |
| Artifacts | One per platform, doubling as the updater payload. | [#17](https://github.com/ai-sirio/sirio/issues/17) |
| Hosting | GitHub Releases on `ai-sirio/sirio`. | [#18](https://github.com/ai-sirio/sirio/issues/18) |
| Discovery | Per-channel static JSON manifest at `https://dl.sirioai.app/`. | [#18](https://github.com/ai-sirio/sirio/issues/18), [#26](https://github.com/ai-sirio/sirio/issues/26) |
| Version line | Continues the Swift app's. Next release `0.6.0`. | [#14](https://github.com/ai-sirio/sirio/issues/14) |
| Channels | Two: Stable and Nightly, compiled in. | [#19](https://github.com/ai-sirio/sirio/issues/19) |
| OS signing | macOS notarized; Windows deferred, ships unsigned until the first public release. | [#15](https://github.com/ai-sirio/sirio/issues/15), [#21](https://github.com/ai-sirio/sirio/issues/21) |
| Payload signing | Ed25519 detached signature, public half compiled in. | [#20](https://github.com/ai-sirio/sirio/issues/20) |
| Applying the swap | Per-platform; never overwrite a running binary, never guess the install root. | [#24](https://github.com/ai-sirio/sirio/issues/24) |
| Rollback | On failure, yes and free. On request, no. | [#80](https://github.com/ai-sirio/sirio/issues/80) |
| User surface | Quiet status-bar indicator, never the tray. | [#88](https://github.com/ai-sirio/sirio/issues/88) |
| Release notes | Stable only, carried in the manifest. | [#89](https://github.com/ai-sirio/sirio/issues/89) |
| App identity | `app.sirioai.sirio`, one identifier for every build. | [#296](https://github.com/ai-sirio/sirio/issues/296) |

## 1. What ships

### 1.1 Target triples

A release covers three combinations, one per operating system
([#16](https://github.com/ai-sirio/sirio/issues/16)):

| OS | Triple |
|---|---|
| macOS | `aarch64-apple-darwin` |
| Windows | `x86_64-pc-windows-msvc` |
| Linux | `x86_64-unknown-linux-gnu` |

Windows targets the **MSVC** ABI, not the GNU one a development machine may
happen to have. The two are not interchangeable — different C runtime, different
linker — and MSVC is the native ABI, is preinstalled on GitHub's
`windows-latest` runners, and is what `Scripts/ci-linux.sh` already cross-checks.

Three combinations are deliberately excluded, none of them forever:
`x86_64-apple-darwin` because Apple has announced macOS 26 as the last release
supporting Intel; `aarch64-pc-windows-msvc` and `aarch64-unknown-linux-gnu`
because nobody has asked and there is no machine to verify them on. The
reasoning is uniform: a combination is not one more build, it is a build, a
signature, a test and an artifact kept alive indefinitely — and the price is
paid at the tenth release, when one crate breaks on one target and the release
must either block or ship an incomplete matrix.

### 1.2 Both platforms were verified by building and running, not by inspection

[#13](https://github.com/ai-sirio/sirio/issues/13) and
[#23](https://github.com/ai-sirio/sirio/issues/23) established that all three
targets link, and that macOS and Windows both run. macOS links nothing outside
the system: `otool -L` lists 23 dynamic libraries and every one is under
`/usr/lib` or `/System/Library`, so nothing third-party travels with the binary.
The window renders, confirmed at the machine.

**A finding that reverses an earlier one, and the reason this spec exists.**
[#13](https://github.com/ai-sirio/sirio/issues/13) measured a `windows-gnu`
build dying at load with `0xC0000135` unless `WebView2Loader.dll` sat beside the
executable, and concluded that a bare `.exe` is not a valid artifact.
[#23](https://github.com/ai-sirio/sirio/issues/23) then measured the **MSVC**
build — the release ABI — and found the opposite: `webview2-com-sys` links
`WebView2LoaderStatic.lib` under MSVC, the import name is absent from the
binary, and moving the DLL away leaves the app starting normally.

**So the Windows artifact does not ship `WebView2Loader.dll`.** The DLL
requirement was a `windows-gnu` fact and does not apply to what is released. The
WebView2 *Runtime* is a separate concern and is handled in §7.3.

### 1.3 The artifacts

One artifact per platform, and **the file a human downloads is the same file the
updater applies** ([#17](https://github.com/ai-sirio/sirio/issues/17)). The cost
is accepted deliberately: no artifact may require a human to click through a
wizard, because an updater cannot.

| Platform | Artifact | Contents | Install target |
|---|---|---|---|
| macOS | `Sirio-<version>.dmg` | `Sirio.app` — `Contents/MacOS/{sirio,sirioctl}`, `Info.plist`, `Resources/icon.icns` | `/Applications` (drag) |
| Windows | `SirioSetup-<version>.exe` — Inno Setup, **per-user** | `sirio.exe`, `sirioctl.exe`, Start Menu entry, uninstaller | `%LOCALAPPDATA%\Programs\Sirio` |
| Linux | `Sirio-<version>-x86_64.AppImage` | one executable bundling GTK3, `libwebkit2gtk-4.1` and their closure | anywhere the user puts it |

**macOS — a `.dmg` around a `.app`.** Notarization is built around bundles: the
ticket staples to the `.app`, so the app stays valid offline. A bare tarball was
rejected even though the binary runs as-built — it gets no icon, no menu-bar
name, no Launch Services registration.

**Windows — a silent-capable per-user installer.** A portable zip is the purer
reading of "one artifact, both roles", but Windows needs two things an archive
cannot give: a canonical install location for the updater to target, and a
bootstrap path for the WebView2 Runtime. Inno Setup over WiX/MSI because an
`.iss` is markedly less work and `/VERYSILENT /NORESTART` is a well-trodden
contract. **Per-user, never per-machine**: a `Program Files` install needs UAC on
every update, defeating the entire reason for choosing a silent installer.

**Linux — AppImage, not a tarball.** This is where the Zed prior art stops
applying: **Zed has no webview, Sirio does.** `wry` links `libwebkit2gtk-4.1`
and GTK3 at load time, so on a host without them Sirio dies before drawing. A
tarball pushes that onto the user and onto every support conversation. The
AppImage bundles it, and gives the simplest swap of the three — replace one file.
It bundles GTK and webkit, **not glibc**: a minimum glibc still applies and
still comes from the build base.

### 1.4 The sibling rule

`resolve_sirioctl_path` finds `sirioctl` as a sibling of the running executable.
Every artifact above therefore places the two binaries in one directory. This is
a constraint on the layout, not a preference.

Two consequences fell out of it
([#17](https://github.com/ai-sirio/sirio/issues/17),
[#25](https://github.com/ai-sirio/sirio/issues/25)):

- **`install_sirioctl` copies rather than symlinks on Linux.** Inside an
  AppImage the binaries live at `/tmp/.mount_SirioXXXXXX/usr/bin/`, a path
  randomised per launch that evaporates when the process exits, so a symlink
  dangles as soon as Sirio quits. Tracked as
  [#100](https://github.com/ai-sirio/sirio/issues/100); deliberately not
  implemented until an AppImage exists to test it against.
- **The Windows copy refreshes itself.** Measured live rather than argued: the
  installed copy had been four days stale and did not recognise a subcommand
  added since, while agent hooks invoked it the whole time. Fixed in PR #79. A
  failed refresh is non-fatal — Windows can lock the destination — and the old
  usable binary is kept for the next launch to retry.
- **macOS gets this free.** A symlink into
  `/Applications/Sirio.app/Contents/MacOS/sirioctl` is a stable path.

### 1.5 Build prerequisites the release job must provide

- **Zig exactly 0.15.2.** `libghostty-vt-sys` shells out to `zig build` and
  upstream pins the version exactly; a *newer* Zig fails too.
- **`sccache` on PATH.** `rust/.cargo/config.toml` sets
  `rustc-wrapper = "sccache"` unconditionally. A runner without it fails before
  compiling a single crate, in a way that looks nothing like its cause.
- **Windows:** Visual Studio Build Tools for the MSVC linker.

## 2. Where it lives

### 2.1 Artifacts: GitHub Releases

Artifacts are published as GitHub Release assets on `ai-sirio/sirio`
([#18](https://github.com/ai-sirio/sirio/issues/18)).

**This depends on the repository being public**, and that is a hard precondition
of the first release rather than a parallel track. A private repo's release
assets return 404 to unauthenticated clients — including through the
`releases/latest/download/` redirect — and a token shipped inside a desktop app
is extractable from the binary and grants read access to the whole private
source. The credential sweep gating the visibility flip is already done.

### 2.2 Discovery: a per-channel static manifest

```
https://dl.sirioai.app/stable.json     <- the only string frozen into the binary
https://dl.sirioai.app/nightly.json
        |
        v  CNAME -> ai-sirio.github.io   (today)
           CNAME -> anything else         (later, without touching installed clients)

manifest -> https://github.com/ai-sirio/sirio/releases/download/v0.6.0/Sirio-0.6.0.dmg
```

**Not the GitHub API.** `/releases/latest` is capped at 60 unauthenticated
requests per hour **per IP**. Every client behind one NAT shares that budget, so
in an office the update check fails for everybody at once, silently, with a 403
nobody sees.

**Not `releases/latest/download/`.** That redirect resolves only to the latest
non-prerelease, so it is single-channel by construction and cannot express
Nightly.

**A custom domain, not `ai-sirio.github.io`.** Only the manifest URL is a
one-way door: it is compiled into every binary and interrogated forever. The
artifact URLs *inside* the manifest are re-read on every check, so artifact
hosting can move at any time without touching a single installed client.
Freezing a hostname we control keeps the one irreversible string behind a DNS
escape hatch.

### 2.3 The domain

`sirioai.app`, registered 2026-08-28 through Spaceship, expiring **2027-08-28**
([#26](https://github.com/ai-sirio/sirio/issues/26)).

`.app` is on the HSTS preload list as a whole TLD, so
`https://dl.sirioai.app/<channel>.json` cannot be downgraded to plaintext by an
on-path attacker — the property that matters most for the one URL §4.2 requires
to carry a signed payload. The corollary is that the name only ever works over
TLS: there is no plaintext fallback if the certificate breaks.

**Renewal is a standing operational risk.** From the first shipped binary
onward, a lapse cannot be corrected in copies already installed. Auto-renew is
the cheap mitigation and should be set now rather than at the first release.

Remaining setup, all of it gated on the repository going public because GitHub
Pages on the free plan requires it:

1. `CNAME dl.sirioai.app` → `ai-sirio.github.io`
2. Set the custom domain in the repository's Pages settings; let it issue the
   TLS certificate.
3. Verify `https://dl.sirioai.app/` serves over HTTPS **before** any release URL
   depends on it.

### 2.4 Artifact filenames are versioned

`Sirio-0.6.0.dmg`, `SirioSetup-0.6.0.exe`, `Sirio-0.6.0-x86_64.AppImage`. Stable
filenames were only ever required by the `latest/download/` shortcut this design
rejects. Versioned names make a downloaded file self-identifying and keep every
past release addressable — which §6.2 relies on.

## 3. Version and channels

### 3.1 The version line continues; the next release is `0.6.0`

The Swift app's tags reach `v0.5.1` and live in the same repository
([#14](https://github.com/ai-sirio/sirio/issues/14)). Publishing `v0.2.0` after
`v0.5.1` would leave `v0.5.1` permanently on top — "latest" in the Releases UI
and the answer any version-ordering tool gives. An updater is a machine that
answers exactly that question, so leaving two plausible candidates for "the
newest version" is the one thing this must not do.

A separate tag prefix (`rust-v0.1.0`) was rejected: two version namespaces in
one repository is the same ambiguity wearing a different coat.

### 3.2 The manifest is the source of truth; the tag derives from it

`[workspace.package] version` in `rust/Cargo.toml` — `0.6.0` today. A shipped
`.app`, `.AppImage` or installed `.exe` has no git repository beside it, so the
running app can only know its version if it was compiled in, and
`CARGO_PKG_VERSION` is what cargo compiles in.

Direction of derivation: **manifest first, tag second.** Releasing `vX.Y.Z`
requires `rust/Cargo.toml` to already declare `X.Y.Z`, and the release job
refuses a tag that disagrees — `Scripts/check-release-version.sh` already
implements this check.

`sirioctl` takes `version.workspace = true` so it cannot lie about which app it
ships beside. The library crates are unpublished internals whose versions
nothing reads; they stay as they are.

The running app surfaces its version twice, from one constant: **in Settings**,
so a user filing a bug can read it without a terminal, and in
**`system.capabilities`** on the control socket, so a script or agent hook can
read it without a UI. `sirioctl version` reports both its own version and the
running app's and warns when they disagree.

### 3.3 Two channels, compiled in

**Stable** — what a user gets, built from a tag. **Nightly** — built from `main`
on a schedule ([#19](https://github.com/ai-sirio/sirio/issues/19)).

Zed runs four channels because Zed has a release team and a preview cohort large
enough to produce signal. Sirio has one maintainer and one download of its last
release. A Preview channel with nobody in it is not a safety net; it is a third
artifact to build, sign, host and reason about.

Nightly earns its place for a different reason: it makes the updater
**observable**. The Zed post-mortem's lesson is that a broken updater is
invisible because a working one is invisible, and a channel that updates every
day is the one place the mechanism gets exercised often enough for silence to
mean something.

**A dev build is not a channel.** It refuses to update at all rather than
pretending to belong somewhere.

The channel is a **compile-time constant**, set from an environment variable the
release job passes (`SIRIO_RELEASE_CHANNEL`), defaulting to a dev value that
disables updating entirely when unset. Runtime discovery was rejected on a
specific failure it invites: a channel read from a settings file is a channel
the user can edit, and the first thing that happens is somebody flips "stable"
to "nightly" in a text file and owns a binary that says stable and polls
nightly.

**Consequence:** a channel switch cannot be an in-app toggle that keeps the
current binary. Switching channels means installing the other channel's
artifact — which is what switching channels *is*.

### 3.4 Poll intervals

Stable checks **daily**, Nightly **hourly**, and both check **at launch**. The
intervals are floors governing a long-running instance; Sirio is a window people
leave open for days, so the launch check alone would be nearly useless. Equally,
an app that must stay light enough for a Raspberry Pi 5 should not wake a radio
every fifteen minutes to learn nothing.

### 3.5 The CI assertion, from the post-mortem

Zed's updater was dead for two releases because a refactor flipped a default to
`false` and nothing noticed. CI must assert that **a Stable build is compiled
with the Stable channel and with updating enabled**. Both halves: a build whose
channel silently fell back to the dev default would also update never, and look
exactly like the bug that started this.

## 4. Signing

Two mechanisms answering different questions, and neither substitutes for the
other. OS code signing proves to *the operating system* who built the app,
before it ever runs. Payload signing proves to *the already-running Sirio* that
what it just downloaded is what we published.

### 4.1 OS-level

**macOS: notarized.** Apple Developer Program is 99 USD/year, identical for
individual and organization enrolment; organization additionally needs a legal
entity, a D-U-N-S number, a domain email and a public website
([#15](https://github.com/ai-sirio/sirio/issues/15)). Notarization runs from CI
via `notarytool`. As of macOS Sequoia the Control-click workaround for an
unnotarized app no longer works — the dialog offers only "Move to Trash" and
"Done" — so notarization is not optional on the reference platform.

**Windows: deferred. Sirio ships unsigned until the first public release**
([#21](https://github.com/ai-sirio/sirio/issues/21)).

The folklore is out of date on the axis that mattered: Microsoft states that EV
certificates no longer bypass SmartScreen, and CSBR §4.1.1 restricts EV issuance
to organizations, so a private individual is ineligible regardless. Two
constraints reshape the cost model: since 2023-06-01 the private key must live
in certified hardware for OV as well as EV, so a `.pfx` in a CI secret is not a
valid configuration; and certificates issued from 2026-03-01 are capped at 460
days, making every renewal a re-key and therefore a CI secret rotation.

The route is determined by one fact — **which legal identity signs**. Azure
Artifact Signing (9.99 USD/month, driven by GitHub OIDC with no long-lived
secret in CI) admits EU *organizations* with three years of tax history but
restricts *individuals* to the US and Canada. An EU-resident individual is
therefore left with a conventional OV certificate at 129–696 USD/year plus
249–1,500 USD of hardware or cloud-HSM attestation, and SSL.com OV with eSigner
is the surveyed pick because a physical token cannot sign from CI at all.

**Consequence to carry: SmartScreen will warn on download** until this is picked
up, and the update path must not assume a signed binary.

**Consequence for §4.2 if the OV route is taken:** CI would hold a credential
that can request a signature. The Ed25519 release key then needs its own home,
and "the same place as the code-signing credential" is the wrong answer — one
compromise would yield both the OS signature and the payload signature, which is
precisely the pair these two mechanisms exist to keep separate.

### 4.2 Payload: Ed25519, verified before anything runs

Zed checks only that the HTTP response succeeded, and this design may not copy
that ([#20](https://github.com/ai-sirio/sirio/issues/20)). Everywhere else in
Sirio a bad byte produces a crash or a wrong pixel; here a bad byte is **code the
app then executes with the user's privileges**. TLS proves you talked to the
right host and nothing about what that host served — and a host is exactly the
thing that gets compromised.

There is a second reason specific to §2.2: artifact URLs were made re-readable
so hosting can move later, and that flexibility is only safe if trust rests on
the payload rather than on where it was fetched from.

- An Ed25519 keypair whose private half never leaves the signing environment.
- The public half **compiled into the binary** beside the manifest URL.
- A detached signature over the artifact bytes, published in the manifest beside
  the hash.

**A hash in the manifest is not a substitute.** The manifest is served over the
same channel by the same authority; whoever can serve a bad artifact can serve a
matching hash. The hash catches corruption, the signature catches substitution.

Note the Linux asymmetry that makes this load-bearing rather than belt-and-braces:
an AppImage the updater swapped in is validated by **nothing** at the OS level.

### 4.3 Key rotation

A binary carries the public key it was compiled with, so a key change cannot
reach installed copies retroactively — an old Sirio trusts the old key forever.

- Binaries carry a **set** of accepted public keys, not one.
- A new key is added to the accepted set **one release before** it starts
  signing, so every copy that can still update has already learned it.
- A **compromised** key is a rebuild-and-republish, not a rotation: a signature
  made with a stolen key verifies perfectly, so the only remedy is to stop
  accepting it and get a build that does not into users' hands — a manual
  announcement, not a mechanism.

Stated rather than hidden: a copy old enough to predate a rotation cannot be
reached by a rotation-aware update, and its user must reinstall by hand. That is
inherent to compiling trust into the binary, and it is why accepted-set-first
sequencing matters — it makes the window narrow instead of unbounded.

### 4.4 When verification fails

The update is discarded and the running app keeps running. No fallback, no "try
anyway" affordance, and the failure is **surfaced rather than silent**: a failed
signature check is either a broken publish or an attack, and both are things the
user must be told about rather than retried around.

## 5. Applying an update

### 5.1 Self-location: derive an install root, and refuse rather than guess

`current_exe()` is the starting point on all three platforms, but only macOS can
use it directly ([#24](https://github.com/ai-sirio/sirio/issues/24)):

- **macOS** — walk up from `current_exe()` to the `.app` bundle
  (`…/Sirio.app/Contents/MacOS/sirio` → `…/Sirio.app`). No bundle above it means
  this is a `cargo run` build, not an install.
- **Windows** — the directory containing `sirio.exe`, expected under
  `%LOCALAPPDATA%\Programs\Sirio`.
- **Linux** — **`$APPIMAGE`, not `current_exe()`.** Inside an AppImage,
  `current_exe()` points into the ephemeral `/tmp/.mount_SirioXXXXXX` squashfs,
  which is not the file to replace and disappears when the process exits.
  `$APPIMAGE` is the only handle on the real file, and **its absence is the
  "this is not an install" signal** on this platform.

When self-location fails: report that the install cannot be located and point at
the download page. Do not guess, do not fall back to a plausible path, do not
attempt a swap. This is also the permanent state of a dev build, so the path is
exercised constantly rather than only in the rare broken case.

### 5.2 The swap: never overwrite a running binary

| Platform | Mechanism |
|---|---|
| macOS | `hdiutil attach` the DMG, replace the installed bundle **move-aside-then-rename**, `hdiutil detach` in a guaranteed-cleanup path. |
| Windows | Re-run `SirioSetup-<version>.exe` with `/VERYSILENT /NORESTART`. **The installer closes Sirio and restarts it.** |
| Linux | Download **beside** the target, `chmod +x`, `rename(2)` over `$APPIMAGE`. |

**macOS:** move-aside-then-rename rather than copy-over, so a failure mid-way
leaves the old bundle intact rather than a half-written one. A leaked mount is a
device the user has to eject by hand, hence the guaranteed-cleanup detach.

**Windows:** a running `.exe` cannot be overwritten. The installer closes the app
rather than a helper applying the swap after exit — a helper is a second thing
that can fail, and it fails *after* the app is gone, with nothing left running
to tell the user. Inno already knows how to close and restart the app it
installed, which is the whole reason §1.3 chose it over a bare archive.

**Linux:** the rename is atomic **on the same filesystem**, which is why the
download must land in the same directory and not `/tmp` — a cross-device rename
is a copy, and a copy can be interrupted halfway through the file the user
launches.

**Only Windows restarts the app.** macOS and Linux apply the swap and let the
running instance carry on until the user relaunches. Killing an editor full of
live agent sessions to install an update the user did not ask to apply right now
is worse than running yesterday's build for another hour.

## 6. Failure and rollback

Two things usually said in one breath, and separating them is what makes this
decidable ([#80](https://github.com/ai-sirio/sirio/issues/80)).

### 6.1 Rollback on failure: yes, and it is free

It is already inside §5.2's mechanisms and only needs stating:

- **macOS** — the moved-aside bundle *is* the previous version. Keep it until
  the new bundle is in place and verified, then delete it. Cost: one bundle's
  disk for the duration of one swap.
- **Linux** — the rename is the only destructive step and it is atomic. A
  failure before it leaves the original untouched. Cost: zero.
- **Windows** — Inno already rolls back its own failed install. Cost: zero, and
  not ours to implement.

**A half-applied update cannot leave the user without a working Sirio on any
platform.** That is the property that actually matters.

### 6.2 Rollback on request: refused

Three reasons that compound:

- **It cannot serve the case it exists for.** The reason to go back is an app too
  broken to be useful — and an app too broken to launch has no UI to offer a
  rollback button.
- **On macOS it does not cover the worst case.** Certificate revocation disables
  copies *already installed*, so if a release is bad because its signature was
  revoked, the retained previous copy is disabled too. The mechanism would be
  absent exactly when the disaster is largest.
- **The disk cost lands continuously on the smallest machine.** A spare bundle or
  AppImage carried forever, plus a pruning rule, is a real tax on a Raspberry Pi
  5's SD card, paid to insure against an event with a working manual remedy.

**The remedy is reinstalling the previous release.** For that to be a real answer
rather than a shrug, two rules follow:

1. **Previous releases stay downloadable**, never deleted when superseded — a
   publishing rule for the release job, not a code change.
2. **The version is readable without launching the app.** §3.2 puts it in
   Settings and in `sirioctl version`, and `sirioctl` keeps working when the GUI
   does not — which is the case that matters here.

Left open deliberately: nothing stops a user reinstalling an older version and
being offered the newer one again on the next check. That is correct default
behaviour. Whether a deliberate stay-behind needs a "skip this version"
affordance belongs to §7's surface.

## 7. What the user sees

### 7.1 The indicator

A small, quiet indicator in the **status bar** — always visible, never modal
([#88](https://github.com/ai-sirio/sirio/issues/88)).

**Not the tray icon.** [#76](https://github.com/ai-sirio/sirio/issues/76) just
made the tray mean *an agent needs you*. Overloading it with *an update exists*
puts two unrelated meanings on one glyph and devalues the urgent one — a user
who learns the badge sometimes means "nothing to do" stops reading it.

**Not a modal.** An update is never more important than the turn an agent is in
the middle of.

### 7.2 The gesture, whose label states the platform's truth

Clicking opens the update detail in Settings, not an immediate install. The
confirming control says what will actually happen, because §5.2 made the
platforms genuinely differ:

- macOS and Linux: **`Update and keep working`**
- Windows: **`Update and restart Sirio`**

One button whose label differs per platform is honest; one label that means two
things is not.

**Download is automatic on Stable**, because §4.2 verifies the signature *after*
download — the bytes must arrive before anything can be checked, so asking first
would only move the cost. It is bounded: once per available version, never
re-attempted on the same version after a failure until the next interval, and
skipped entirely on a metered connection where the platform exposes that. An
updater that retries a failed 80 MB download in a loop is a battery bug.

**The opt-out is a real off**, per install, in Settings → General: no polling, no
download, no indicator. Not "notify but do not install" — a third state that
reads as off to the user and behaves as on to the battery. Turning it off does
not hide the app's own version, which §3.2 surfaces independently.

### 7.3 The `last checked` timestamp

Settings → General shows `Checked 14 minutes ago` beside the version, and
`Could not check for updates` with the time of the attempt when a check fails —
never silence.

This is the user-visible half of the Zed post-mortem's lesson. A working updater
is invisible, which is exactly why its *liveness* must be visible somewhere
cheap: a timestamp stuck at three weeks ago is a bug report a user can file
without knowing anything about release channels. It pairs with §3.5 — CI proves
the build *intends* to update, the timestamp proves it *is*.

### 7.4 Release notes

**Stable shows notes. Nightly does not**, and says so once: `Nightly builds
track main` in place of the section
([#89](https://github.com/ai-sirio/sirio/issues/89)). A nightly's honest note is
"today's `main`", and printing that daily trains the user to skip the section —
which then costs them the one time Stable has something worth reading.

The text ships **inside the manifest**, in a `notes` field beside the version and
artifact URLs, so it costs zero additional requests and needs no public repo to
link to.

**Hand-written, seeded from `Scripts/generate-changelog.sh`.** The repo enforces
Conventional Commits so the script produces a decent skeleton, but a raw list of
`fix(tray): …` lines is a diff, not a message — half the commits in a release
are refactors and test scaffolding no user should read past. The script stops
being a publishing tool and becomes what it is good at: making sure nothing is
forgotten.

**No `CHANGELOG.md` in the tree.** A file that must be updated in the same commit
as a release goes stale the first busy week; the manifest is already the thing
that must be correct for the release to work at all.

**Absent notes render nothing** — no empty panel, no spinner, no "no release
notes available". An update without notes is normal, and a placeholder makes
normal look broken. Length is bounded: the manifest is fetched on a schedule by
every installed copy and is not a place for an essay.

### 7.5 The WebView2 Runtime on Windows

Distinct from the loader shim of §1.2 — the shim is statically linked, the
runtime is a machine-level Microsoft component
([#85](https://github.com/ai-sirio/sirio/issues/85)).

The Inno installer chains the WebView2 bootstrapper **only when the runtime is
absent**, silently, and treats its failure as **non-fatal**:

- **Conditionally**, because the check is a registry lookup
  (`…\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}`, `pv`) and
  Windows 11 ships the runtime preinstalled, so on most machines this resolves
  without touching the network. Skipping the check *because* it is Windows 11
  was rejected: a user can uninstall the runtime.
- **Non-fatally**, because the failure it most often signals is "no network right
  now". Terminal, chat, git and settings all work without WebView2; only the
  browser surface does not.
- **Per-user**, matching the never-elevating installer. The honest cost: on a
  shared machine each account fetches its own copy.

When the runtime is missing anyway, **the browser surface says so in place of its
content** — that it needs the Microsoft Edge WebView2 Runtime, that Sirio could
not install it, and one action to retry. Not an empty pane, not a crash.

**No standalone installer is shipped** — tens of megabytes in an artifact that
doubles as the updater payload, to serve machines that are offline at install
time *and* lack the runtime.

**The update path does not re-check.** An update replaces Sirio, not the
machine's runtime; the check belongs to first install only, and §5.2's swap must
not acquire it.

## 8. App identity

### 8.1 The bundle identifier is `app.sirioai.sirio`

One identifier, for release and dev builds alike
([#296](https://github.com/ai-sirio/sirio/issues/296)). Reverse-DNS on
`sirioai.app`, the only domain we own.

It replaces three prefixes that were live in the tree simultaneously, two of
which asserted reverse-DNS over domains registered to third parties:

| Was | Where |
|---|---|
| `dev.sirio.Sirio` | `Scripts/build-app-bundle.sh:81` |
| `dev.sirio.Sirio` | `2026-08-29-macos-release-platform-design.md:133` |
| `dev.sirio.sirio-dev` | `Scripts/build-dev.sh:38` |
| `ai.sirio.Sirio` | `docs/kitty-graphics-spec.md:75,96` |

`app.` in leading position reads oddly and the leaf is lower-case rather than
Apple's capitalised convention. Both were weighed and accepted: nobody reads this
string but the OS, and matching a domain we control is the whole point of the
convention.

**Dev builds share it.** The known cost is developer-facing only: a dev build and
an installed release carry the same identifier from different paths, so macOS may
re-prompt for TCC grants when switching between them.

### 8.2 The Windows installer's `AppId` is a fixed GUID

```
581478B4-5A82-4E6D-9886-E3470E3A663C
```

The `.iss` quotes it verbatim and **it must never change.** Inno decides
upgrade-in-place versus side-by-side by comparing `AppId`, and §5.2's update path
re-runs the installer precisely *expecting* an in-place upgrade. If that string
moves, users end up with two Sirios installed.

This is the one deliberate exception to the one-identity-string rule of §8.4. The
reasoning is this effort's own recent history: the Tiller → Sirio rename proved
names move, and this is the single field where moving means duplicate
installations on machines already in the field. A GUID has nothing to rename.

### 8.3 Display name and `.desktop`

Display name is **`Sirio`** on every platform — `Sirio.app` is the filesystem's
convention, not a different name, and the `.desktop` `Name=` is `Sirio`
([#86](https://github.com/ai-sirio/sirio/issues/86)). A name that differs per
platform is a name users cannot search for.

**`StartupWMClass` must be measured, not guessed.** It has to match what the
window actually reports or GNOME shows a second, generic icon in the dash beside
the running app. It is one `xprop WM_CLASS` on a running Linux build; record the
measured value in the `.desktop` file's own comment.

### 8.4 The packaging files are generated from one source

`Info.plist`, the Inno `.iss` and the `.desktop` entry repeat the same four
facts — identifier, name, version, icon path. Three hand-maintained copies of
four facts is the shape that drifts, especially now that §3.2 has made the
version a single source of truth that would otherwise be transcribed three times
by hand.

Templates live in the repo with placeholders and are filled by the release job
from `Cargo.toml`'s version and one small identity file. The templates are
committed and reviewable; the filled artifacts are not.

### 8.5 State directories

`#86` decided to leave the then-current `TillerRust` paths alone because moving
them is a migration rather than a rename. **The rebrand has since moved them
anyway**, and the migration was written:
`sirio/src/session.rs:migrate_legacy_state_root` renames a pre-rebrand
`TillerRust` root to `Sirio` once, atomically, and is a no-op on every later
launch. Nothing further is owed here; the rule that survives is that no *third*
spelling may appear.

## 9. Rules for the release job

Consequences that are publishing discipline rather than code, collected because
each was decided somewhere else and none of them is enforced by anything today.

1. **Upload every asset first, write the manifest last.** A manifest published
   early advertises a version whose artifacts are not fully uploaded, and the
   updater downloads a 404. (§2.2)
2. **Refuse a tag that disagrees with `rust/Cargo.toml`.** (§3.2)
3. **Never delete a superseded release.** Reinstalling the previous version is
   the only rollback path there is. (§6.2)
4. **Assert in CI that a Stable build carries the Stable channel with updating
   enabled.** (§3.5)
5. **Provide Zig 0.15.2 and `sccache` on every runner.** (§1.5)
6. **Generate the three packaging files from one identity source**, never by
   hand. (§8.4)

## 10. What this spec does not cover

- **Implementation.** This document is the handoff; building the updater is the
  effort that follows it.
- **Native package ecosystems** — Homebrew, winget, AUR, deb, Flatpak. The shape
  decision picked a single in-app updater; publishing into ecosystems that update
  on their own is a separate effort. Flatpak, Snap and MSIX are additionally
  hostile to Sirio's activity detection (which walks `/proc` for the pane shell's
  child processes) and to its control socket in `$XDG_RUNTIME_DIR`.
- **Migrating users of the retired Swift app.** Its last release has a download
  count of 1; there is no installed base to carry across.
- **[#100](https://github.com/ai-sirio/sirio/issues/100)** — refreshing
  `sirioctl` from inside an AppImage, deliberately deferred until an AppImage
  exists to test against.
- **[#298](https://github.com/ai-sirio/sirio/issues/298)** — the macOS cookie
  regression this effort surfaced. Product work, not a step toward release.

## Ticket index

Every decision above, with the ticket that holds its reasoning.

| Ticket | Question |
|---|---|
| [#13](https://github.com/ai-sirio/sirio/issues/13) | Does it link and run on macOS and Windows? |
| [#14](https://github.com/ai-sirio/sirio/issues/14) | What version line does the Rust port carry? |
| [#15](https://github.com/ai-sirio/sirio/issues/15) | What do notarization and code signing require, and cost? |
| [#16](https://github.com/ai-sirio/sirio/issues/16) | Which OS and architecture combinations? |
| [#17](https://github.com/ai-sirio/sirio/issues/17) | What is a release artifact on each platform? |
| [#18](https://github.com/ai-sirio/sirio/issues/18) | Where do artifacts and the manifest live? |
| [#19](https://github.com/ai-sirio/sirio/issues/19) | How many channels, and how does a build know which it is? |
| [#20](https://github.com/ai-sirio/sirio/issues/20) | Is the payload verified beyond HTTPS? |
| [#21](https://github.com/ai-sirio/sirio/issues/21) | Which Windows signing route? |
| [#23](https://github.com/ai-sirio/sirio/issues/23) | Does it link and run on `x86_64-pc-windows-msvc`? |
| [#24](https://github.com/ai-sirio/sirio/issues/24) | How does an instance locate itself and apply the swap? |
| [#25](https://github.com/ai-sirio/sirio/issues/25) | How does an update refresh the installed `sirioctl`? |
| [#26](https://github.com/ai-sirio/sirio/issues/26) | Which domain serves the manifest? |
| [#80](https://github.com/ai-sirio/sirio/issues/80) | Does an update keep the artifact it replaced? |
| [#85](https://github.com/ai-sirio/sirio/issues/85) | Does the installer bootstrap the WebView2 Runtime? |
| [#86](https://github.com/ai-sirio/sirio/issues/86) | What is Sirio's identity to each OS? |
| [#88](https://github.com/ai-sirio/sirio/issues/88) | How does a running Sirio surface an available update? |
| [#89](https://github.com/ai-sirio/sirio/issues/89) | Does an update say what changed? |
| [#296](https://github.com/ai-sirio/sirio/issues/296) | What is the bundle identifier? |
